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
    srv.wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            srv.kill();
            panic!("service server never became ready")
        });

    // Async client runs once, awaits the reply, logs it, exits.
    let mut cli = spawn(&client, &locator, "async-service-client");
    let out = cli
        .wait_for_all_output(Duration::from_secs(30))
        .unwrap_or_default();
    srv.kill();

    assert!(
        out.contains("Result of add_two_ints:"),
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
    srv.wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            srv.kill();
            panic!("action server never became ready")
        });

    let mut cli = spawn(&client, &locator, "async-action-client");
    let out = cli
        .wait_for_all_output(Duration::from_secs(40))
        .unwrap_or_default();
    srv.kill();

    assert!(
        out.contains("Goal accepted by server"),
        "async action client never resolved its awaited goal acceptance — the \
         tokio spin_async + .await path did not complete against the server.\n{out}"
    );
}
