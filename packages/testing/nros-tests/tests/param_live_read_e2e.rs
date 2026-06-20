//! Phase 264 W4c — runtime E2E for the declarative in-callback parameter read
//! (nros↔nros, no ROS 2 required).
//!
//! The `ws-params-rust` workspace node (`param_talker`) reads its parameter LIVE every
//! tick via `ctx.parameter::<i64>("publish_period_ms")` and publishes that value on
//! `/chatter`. The launch-baked initial is 250, so a correctly-wired W4c read makes a
//! cross-process nros subscriber observe `Received: 250` — proving the whole chain:
//! `[param_services]` seeds the volatile store → the component cell captures the store
//! pointer at registration → `dispatch_into_cell` threads it onto `CallbackCtx` →
//! `ctx.parameter` reads the live value → it reaches the wire.
//!
//! The `ros2 param set` reconfig half (which needs a wire-matched `rmw_zenoh_cpp`
//! overlay) lives in `tests/params.rs` (the ROS 2 interop lane).
//!
//! Run with: `cargo nextest run -p nros-tests --test param_live_read_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener, build_native_workspace_rust_params_entry,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn the `param_talker` workspace entry on `locator`, spinning for `spin_ms`.
fn spawn_param_entry(locator: &str, spin_ms: u32) -> ManagedProcess {
    let entry = build_native_workspace_rust_params_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("params workspace entry fixture not built: {e}"));
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, "param_talker").expect("spawn param_talker entry")
}

/// Spawn an nros `/chatter` subscriber (prints `Received: <data>` per message).
fn spawn_listener(locator: &str) -> ManagedProcess {
    let listener = build_native_listener()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client");
    let mut proc = ManagedProcess::spawn_command(cmd, "listener").expect("spawn listener");
    // Subscription must be live before the talker publishes.
    proc.wait_for_output_pattern("Listener", Duration::from_secs(8))
        .expect("listener did not become ready");
    proc
}

/// W4c — the node reads the launch-baked initial (`250`) LIVE in its callback via
/// `ctx.parameter::<i64>` and publishes it; a cross-process nros subscriber must see it.
#[rstest]
fn param_live_read_publishes_baked_initial(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let locator = zenohd_unique.locator();

    let mut listener = spawn_listener(&locator);
    let mut entry = spawn_param_entry(&locator, 8000);

    // The published value IS the live param read. The baked initial is 250, so a node
    // that wired `ctx.parameter` correctly publishes "Received: 250" on the subscriber.
    let out = listener
        .wait_for_output_count("Received: 250", 3, Duration::from_secs(15))
        .unwrap_or_else(|_| {
            entry.kill();
            listener.kill();
            panic!(
                "subscriber never saw the live-read baked param value (250) on /chatter — \
                 `ctx.parameter::<i64>(\"publish_period_ms\")` did not reach the callback"
            )
        });

    entry.kill();
    listener.kill();

    let n = nros_tests::count_pattern(&out, "Received: 250");
    assert!(n >= 3, "expected ≥3 live-read publishes of 250, got {n}");
}
