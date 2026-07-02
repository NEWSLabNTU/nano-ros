//! Phase 269 W1 — E2E for C/C++ in-callback live parameter read.
//!
//! The `ws-params-c` / `ws-params-cpp` workspace entries boot a single node that:
//! 1. Gets `publish_period_ms = 250` seeded into the executor's volatile store by the
//!    generated `nros_cpp_declare_param` call in `__nros_entry_setup` (emit_c/cpp.rs W1).
//! 2. Reads that value LIVE each tick via `nros_cpp_get_param_integer` (both C and C++)
//!    and publishes it on `/chatter`.
//!
//! A cross-process nros listener must see `Received: 250` ≥3 times — proving the full
//! chain: emit seeds param → store holds it → component reads live → reaches the wire.
//!
//! `ros2 param set` reconfig half lives in the ROS 2 interop lane (needs rmw_zenoh_cpp).
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_c_param_live_read_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink, build_native_workspace_c_params_entry,
    build_native_workspace_cpp_params_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

fn spawn_entry(path: PathBuf, label: &str, locator: &str, spin_ms: u32) -> ManagedProcess {
    let mut cmd = Command::new(path);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, label).expect("spawn entry")
}

fn spawn_listener(locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client");
    let mut proc = ManagedProcess::spawn_command(cmd, "listener").expect("spawn listener");
    proc.wait_for_output_pattern("Listener", Duration::from_secs(8))
        .expect("listener did not become ready");
    proc
}

/// C component reads the launch-baked initial (250) LIVE via nros_cpp_get_param_integer.
#[rstest]
fn c_param_live_read_publishes_baked_initial(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let path = build_native_workspace_c_params_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("ws-params-c entry fixture not built: {e}"));

    let locator = zenohd_unique.locator();
    let mut listener = spawn_listener(&locator);
    let mut entry = spawn_entry(path, "c_param_talker", &locator, 8000);

    let out = listener
        .wait_for_output_count(
            nros_tests::output::int32_listener_line(250).as_str(),
            3,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            entry.kill();
            listener.kill();
            panic!(
                "C component never published live-read baked param (250) on /chatter — \
                 nros_cpp_get_param_integer did not reach the callback"
            )
        });

    entry.kill();
    listener.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::int32_listener_line(250).as_str());
    assert!(n >= 3, "expected ≥3 live-read publishes of 250, got {n}");
}

/// C++ component reads the launch-baked initial (250) LIVE via nros_cpp_get_param_integer
/// on the executor handle saved from node.executor_handle() at configure time.
#[rstest]
fn cpp_param_live_read_publishes_baked_initial(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let path = build_native_workspace_cpp_params_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("ws-params-cpp entry fixture not built: {e}"));

    let locator = zenohd_unique.locator();
    let mut listener = spawn_listener(&locator);
    let mut entry = spawn_entry(path, "cpp_param_talker", &locator, 8000);

    let out = listener
        .wait_for_output_count(
            nros_tests::output::int32_listener_line(250).as_str(),
            3,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            entry.kill();
            listener.kill();
            panic!(
                "C++ component never published live-read baked param (250) on /chatter — \
                 nros_cpp_get_param_integer on executor_handle did not reach the callback"
            )
        });

    entry.kill();
    listener.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::int32_listener_line(250).as_str());
    assert!(n >= 3, "expected ≥3 live-read publishes of 250, got {n}");
}
