//! phase-263 A1 (Track D) — runtime E2E for the declarative `AddTwoInts` service
//! round-trip, cross-process (nros↔nros, no ROS 2 required).
//!
//! In-process (same-executor) service server+client do not talk — the server never
//! receives the locally-issued query (issue 0096). So the service demo runs the
//! server and client as two separate processes, the supported topology (matching the
//! imperative `native_api.rs::test_native_service_communication`):
//!
//!   - `native_service_server_entry` boots `add_server` (the `AddTwoInts` server on
//!     /add_two_ints) via `nros::main!`.
//!   - `native_service_client_entry` boots `add_client`, which each second issues a
//!     blocking `AddTwoInts` call (`a`,`1`) with `a` counting up from 0, then
//!     republishes the server's reply `sum` on `/sum`.
//!
//! An external nros subscriber on `/sum` therefore observes the server-computed sums
//! `1, 2, 3, …` — proving the full declarative service chain end-to-end across the
//! process boundary: client `call_for_name` → server `on_callback` sum → reply →
//! client publish. If the round-trip were broken, nothing would reach `/sum`.
//!
//! Run with: `cargo nextest run -p nros-tests --test service_roundtrip_xprocess_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink,
    build_native_workspace_rust_service_client_entry,
    build_native_workspace_rust_service_server_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn a workspace service entry binary on `locator`, spinning for `spin_ms`.
fn spawn_entry(
    entry: std::path::PathBuf,
    label: &'static str,
    locator: &str,
    spin_ms: u32,
) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

/// Spawn an nros subscriber on `/sum` (prints `Received: <sum>` per message).
fn spawn_sum_listener(locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", "/sum");
    let mut proc = ManagedProcess::spawn_command(cmd, "sum-listener").expect("spawn sum listener");
    // The subscription must be live before add_client republishes the first sum.
    proc.wait_for_output_pattern("Listener", Duration::from_secs(8))
        .expect("sum listener did not become ready");
    proc
}

/// A1 — the workspace service round-trip is observable end-to-end across processes:
/// `add_client` calls `add_server`, and the server-computed sums `1, 2, 3` reach a
/// `/sum` subscriber in order.
#[rstest]
fn service_roundtrip_publishes_server_computed_sums(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let server = build_native_workspace_rust_service_server_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("service server entry fixture not built: {e}"));
    let client = build_native_workspace_rust_service_client_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("service client entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut listener = spawn_sum_listener(&locator);
    let mut srv = spawn_entry(server, "service-server", &locator, 16000);
    // Give the server a moment to register its queryable before the client calls.
    std::thread::sleep(Duration::from_millis(1000));
    let mut cli = spawn_entry(client, "service-client", &locator, 16000);

    // add_client publishes a+1 at 1 Hz with a = 0,1,2,… → the server returns
    // 1,2,3,…; seeing the third confirms ≥3 successful cross-process round-trips.
    let out = listener
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(22),
        )
        .unwrap_or_else(|_| {
            cli.kill();
            srv.kill();
            listener.kill();
            panic!(
                "/sum subscriber never saw 3 server-computed sums — the cross-process \
                 AddTwoInts service round-trip (add_client → add_server → /sum) did not complete"
            )
        });

    cli.kill();
    srv.kill();
    listener.kill();

    // The first three sums must be exactly the server-computed 1, 2, 3 (a=0,1,2 + b=1).
    for n in [1, 2, 3] {
        let expected = nros_tests::output::int32_listener_line(n);
        assert!(
            out.contains(expected.as_str()),
            "expected server-computed sum line {expected:?} on /sum, got:\n{out}"
        );
    }
}
