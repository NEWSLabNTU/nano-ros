//! issue 0096 (regression guard) — runtime E2E for the IN-PROCESS (same-executor,
//! same zenoh session) declarative `AddTwoInts` service round-trip.
//!
//! This is the in-process sibling of `roundtrip_xprocess_e2e` (once
//! ~~`service_roundtrip_xprocess_e2e`~~). Where that test
//! boots the server and client as two SEPARATE processes (the cross-process topology that
//! always worked), this one boots BOTH `add_server` and `add_client` on one executor via
//! `linux_service_inprocess_entry` (`service_inprocess.launch.xml`) — a single process
//! sharing one zenoh-pico session.
//!
//! `add_client` is the ROS 2 tutorial client (phase-535 W0.b): it calls `add_server` ON THE
//! SAME SESSION once, with 2 + 3, and prints `Result of add_two_ints: 5` only when the call
//! returns `Ok`. That line therefore appears if and only if the same-session
//! query→queryable round-trip completes. (Until phase-535 the client called every second and
//! republished each sum on `/sum` for an external subscriber; the line it prints itself is the
//! same evidence with one process fewer.)
//!
//! Before the fix, zenoh-pico's `Z_FEATURE_LOCAL_QUERYABLE` / `Z_FEATURE_LOCAL_SUBSCRIBER`
//! loopback was compiled out (hardcoded `0`), so the locally-issued query never reached the
//! same-session queryable: `call_for_name` timed out, no result was printed, and this test
//! fails. Enabling the loopback for the host build (`nros-zpico-build`) restores delivery.
//!
//! Run with: `cargo nextest run -p nros-tests --test service_roundtrip_inprocess_e2e`

use nros_tests::fixtures::{
    ManagedProcess, RequireFixture, ZenohRouter,
    build_native_workspace_rust_service_inprocess_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// issue 0096 — the WHOLE service round-trip happens inside ONE process/executor:
/// `add_client` calls `add_server` on the same zenoh session and prints the
/// server-computed sum. If same-session delivery were broken (the pre-fix state),
/// `call_for_name` would time out and no result line would appear.
#[rstest]
fn inprocess_service_roundtrip_publishes_server_computed_sums(zenohd_unique: ZenohRouter) {
    require_zenohd();
    let entry = build_native_workspace_rust_service_inprocess_entry()
        .map(|p| p.to_path_buf())
        .require("in-process service entry");
    let locator = zenohd_unique.locator();

    // Boot the single in-process entry (server + client on one executor/session).
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "20000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut entry_proc =
        ManagedProcess::spawn_image(cmd, "service-inprocess").expect("spawn in-process entry");

    let out = entry_proc
        .wait_for_output_pattern(
            nros_tests::output::SERVICE_RESULT_PREFIX,
            Duration::from_secs(25),
        )
        .unwrap_or_else(|_| {
            entry_proc.kill();
            panic!(
                "add_client never printed its result — the IN-PROCESS (same-session) \
                 AddTwoInts round-trip (add_client → add_server) did not complete \
                 (issue 0096: zenoh-pico same-session loopback)"
            )
        });
    entry_proc.kill();

    // The tutorial request is 2 + 3; the server computes the sum.
    let expected = nros_tests::output::service_result_line(5);
    assert!(
        out.contains(expected.as_str()),
        "expected the server-computed result line {expected:?}, got:\n{out}"
    );
}
