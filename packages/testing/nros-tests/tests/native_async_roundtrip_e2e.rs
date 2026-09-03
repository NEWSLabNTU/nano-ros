//! #102 H3 (phase-284 W2) — runtime e2e for the ASYNC native client examples.
//!
//! `native/rust/{service,action}-client-async` are the tokio-flavoured client
//! variants: they move the executor into a background `spin_async()` task and
//! `.await` the goal/response Promises directly (the action one also streams
//! feedback with `futures::StreamExt`). Phase-275 gave them a compile-check
//! fixture row; this proves the async pattern actually RESOLVES its awaited
//! Promises against a live server — the distinguishing behaviour the sync
//! roundtrip tests don't exercise.
//!
//! Each pairs the async client with the matching SYNC native server over a
//! private zenohd (no ROS 2 needed). Servers spin until killed; the async
//! clients run once and exit after the await completes.
//!
//! Run: `cargo nextest run -p nros-tests --test native_async_roundtrip_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_action_server, build_native_async_action_client,
    build_native_async_service_client, build_native_service_server, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

fn spawn(bin: &std::path::Path, locator: &str, label: &'static str) -> ManagedProcess {
    let mut cmd = Command::new(bin);
    cmd.env("RUST_LOG", "info").env("NROS_LOCATOR", locator);
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

/// H3 — async service client `.await`s the `AddTwoInts` reply from the native
/// service server.
#[rstest]
fn native_async_service_client_awaits_reply(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let server = build_native_service_server()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native service-server fixture not built: {e}"));
    let client = build_native_async_service_client()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| {
            nros_tests::skip!("native async service-client fixture not built: {e}")
        });
    let locator = zenohd_unique.locator();

    // Server first, so its queryable is discoverable before the client calls.
    let mut srv = spawn(&server, &locator, "service-server");
    srv.wait_for_output_pattern(
        nros_tests::output::SERVICE_SERVER_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| {
        srv.kill();
        panic!("service server never became ready")
    });

    // Async client runs once, awaits the reply, logs it, exits.
    //
    // issue 1026 — the reply line is this client's TERMINAL event, so wait for
    // it rather than draining 30 s and killing whatever is left. Same
    // assertion, but it now returns the moment the awaited Promise resolves
    // and the failure carries the full transcript instead of the last window.
    let mut cli = spawn(&client, &locator, "async-service-client");
    let out = cli.collect_until(
        nros_tests::output::SERVICE_RESULT_PREFIX,
        Duration::from_secs(30),
    );
    srv.kill();

    assert!(
        out.contains(nros_tests::output::SERVICE_RESULT_PREFIX),
        "async service client never resolved its awaited reply — the tokio \
         spin_async + .await path did not complete against the server.\n{out}"
    );
}

/// H3 — async action client `.await`s goal acceptance + result from the native
/// action server (and streams feedback via `StreamExt`).
#[rstest]
fn native_async_action_client_awaits_goal_and_result(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let server = build_native_action_server()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native action-server fixture not built: {e}"));
    let client = build_native_async_action_client()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native async action-client fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut srv = spawn(&server, &locator, "action-server");
    srv.wait_for_output_pattern(
        nros_tests::output::ACTION_SERVER_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| {
        srv.kill();
        panic!("action server never became ready")
    });

    // issue 1026 — this waited 40 s, killed the client, and then asserted only
    // GOAL ACCEPTANCE, which is a MID-RUN marker: the client awaits acceptance,
    // then streams feedback, then awaits the result. A hang anywhere after
    // acceptance — a feedback stream that never yields, a `get_result` Promise
    // that never resolves — printed the accepted line and reported PASS.
    //
    // So the wait and the assertion now name the client's TERMINAL event, the
    // result line, and acceptance is asserted on the way there.
    //
    // Bound stated: one goal. Feedback CONTENT is not asserted here (the
    // Fibonacci sequence is checked by the sync action cells); what this adds
    // is that the async `.await` path reaches its end, not just its middle.
    let mut cli = spawn(&client, &locator, "async-action-client");
    let out = cli.collect_until(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(40),
    );
    srv.kill();

    assert!(
        out.contains(nros_tests::output::ACTION_GOAL_ACCEPTED_MARKER),
        "async action client never resolved its awaited goal acceptance — the \
         tokio spin_async + .await path did not complete against the server.\n{out}"
    );
    assert!(
        out.contains(nros_tests::output::ACTION_RESULT_PREFIX),
        "async action client accepted the goal but never resolved its awaited \
         RESULT — the .await path stalled after acceptance (feedback stream or \
         get_result), which the old goal-acceptance-only assertion reported as \
         a pass.\n{out}"
    );
}
