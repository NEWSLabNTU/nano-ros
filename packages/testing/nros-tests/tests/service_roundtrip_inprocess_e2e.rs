//! issue 0096 (regression guard) — runtime E2E for the IN-PROCESS (same-executor,
//! same zenoh session) declarative `AddTwoInts` service round-trip.
//!
//! This is the in-process sibling of `service_roundtrip_xprocess_e2e`. Where that test
//! boots the server and client as two SEPARATE processes (the cross-process topology that
//! always worked), this one boots BOTH `add_server` and `add_client` on one executor via
//! `native_service_inprocess_entry` (`service_inprocess.launch.xml`) — a single process
//! sharing one zenoh-pico session.
//!
//! `add_client` issues a blocking `AddTwoInts` call (`a`,`1`, `a` counting up from 0) to
//! `add_server` ON THE SAME SESSION each second, and republishes the server-computed `sum`
//! on `/sum` ONLY when the call returns `Ok` (`if let Ok(resp) = call_for_name(...)`). An
//! external `/sum` subscriber therefore observes `1, 2, 3, …` if and only if the
//! same-session query→queryable round-trip completes.
//!
//! Before the fix, zenoh-pico's `Z_FEATURE_LOCAL_QUERYABLE` / `Z_FEATURE_LOCAL_SUBSCRIBER`
//! loopback was compiled out (hardcoded `0`), so the locally-issued query never reached the
//! same-session queryable: `call_for_name` timed out, nothing reached `/sum`, and this test
//! fails. Enabling the loopback for the host build (`nros-zpico-build`) restores delivery.
//!
//! Run with: `cargo nextest run -p nros-tests --test service_roundtrip_inprocess_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink,
    build_native_workspace_rust_service_inprocess_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

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

/// issue 0096 — the WHOLE service round-trip happens inside ONE process/executor:
/// `add_client` calls `add_server` on the same zenoh session, and the server-computed
/// sums `1, 2, 3` reach an external `/sum` subscriber in order. If same-session delivery
/// were broken (the pre-fix state), `call_for_name` would time out and `/sum` would stay
/// silent.
#[rstest]
fn inprocess_service_roundtrip_publishes_server_computed_sums(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_rust_service_inprocess_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("in-process service entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut listener = spawn_sum_listener(&locator);

    // Boot the single in-process entry (server + client on one executor/session).
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "20000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut entry_proc =
        ManagedProcess::spawn_command(cmd, "service-inprocess").expect("spawn in-process entry");

    // add_client publishes a+1 at 1 Hz with a = 0,1,2,… → the server returns 1,2,3,…;
    // seeing the third confirms ≥3 successful same-session round-trips.
    let out = listener
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(25),
        )
        .unwrap_or_else(|_| {
            entry_proc.kill();
            listener.kill();
            panic!(
                "/sum subscriber never saw 3 server-computed sums — the IN-PROCESS \
                 (same-session) AddTwoInts round-trip (add_client → add_server → /sum) did \
                 not complete (issue 0096: zenoh-pico same-session loopback)"
            )
        });

    entry_proc.kill();
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
