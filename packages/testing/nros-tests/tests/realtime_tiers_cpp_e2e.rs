//! Phase 269 W4 — runtime E2E for the 2-tier sched-context C++ workspace
//! (`ws-realtime-cpp`, nros↔nros, no ROS 2 required).
//!
//! `examples/workspaces/ws-realtime-cpp` declares two C++ configure-shape
//! components, each with a cmake `CALLBACK_GROUPS` declaration. `system.toml`
//! maps those groups to two priority tiers (`[tiers.high]` / `[tiers.low]`)
//! via `[[node_overrides]]`. The codegen emits `nros_cpp_create_sched_context` +
//! `NodeBuilder::sched()` calls to bind each node to its posix-priority sched
//! context (RFC-0015 §4.2).
//!
//! The high-tier `ctrl_node` publishes a counter on `/ctrl` every 10 ms; the
//! low-tier `telem_node` publishes on `/telem` every 100 ms. Two external nros
//! subscribers prove **both tiers are scheduled and run at their distinct cadences**:
//! the high-tier `/ctrl` publishes at roughly 10× the low-tier `/telem` rate.
//!
//! (Tier *priority* preemption is advisory on native — real priority tasks on an
//! RTOS deploy; this asserts both tiers run at their declared periods, the W4 C++
//! sched-context deliverable.)
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_cpp_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener, build_native_workspace_cpp_realtime_entry,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn an nros subscriber on `topic` (prints `Received: <n>` per message).
fn spawn_listener(topic: &'static str, locator: &str) -> ManagedProcess {
    let listener = build_native_listener()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", topic);
    let mut proc =
        ManagedProcess::spawn_command(cmd, topic).unwrap_or_else(|e| panic!("spawn {topic}: {e}"));
    proc.wait_for_output_pattern("Listener", Duration::from_secs(8))
        .unwrap_or_else(|_| panic!("{topic} listener did not become ready"));
    proc
}

/// W4 C++ — the 2-tier sched-context entry schedules both tiers at their declared
/// cadences: the 10 ms high-tier `/ctrl` node publishes far more often than the
/// 100 ms low-tier `/telem` node.
#[rstest]
fn realtime_tiers_cpp_schedule_high_and_low(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_cpp_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("ws-realtime-cpp entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut ctrl = spawn_listener("/ctrl", &locator);
    let mut telem = spawn_listener("/telem", &locator);

    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "12000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "5");
    let mut proc =
        ManagedProcess::spawn_command(cmd, "realtime-cpp").expect("spawn realtime-cpp entry");

    // Anchor on the SLOW tier: once telem (100 ms) has published 5 times, enough
    // wall time (~0.5 s+) has elapsed that the 10 ms ctrl tier should have
    // published many more — proving both tiers are live and the high tier runs faster.
    let telem_out = telem
        .wait_for_output_count("Received:", 5, Duration::from_secs(20))
        .unwrap_or_else(|_| {
            proc.kill();
            ctrl.kill();
            telem.kill();
            panic!("low-tier /telem never reached 5 publishes — the low tier was not scheduled")
        });
    let ctrl_out = ctrl
        .wait_for_output_count("Received:", 1, Duration::from_secs(2))
        .unwrap_or_else(|_| {
            proc.kill();
            ctrl.kill();
            telem.kill();
            panic!("high-tier /ctrl produced nothing — the high tier was not scheduled")
        });

    proc.kill();
    ctrl.kill();
    telem.kill();

    let telem_n = nros_tests::count_pattern(&telem_out, "Received:");
    let ctrl_n = nros_tests::count_pattern(&ctrl_out, "Received:");

    assert!(
        telem_n >= 5,
        "expected ≥5 low-tier /telem publishes, got {telem_n}"
    );
    // 10 ms vs 100 ms ⇒ ~10×; assert a clear ≥3× margin to stay robust against
    // native timer jitter and zenoh delivery batching.
    assert!(
        ctrl_n >= telem_n * 3,
        "expected the high tier (/ctrl, 10 ms) to publish ≥3× the low tier (/telem, 100 ms): \
         ctrl={ctrl_n} telem={telem_n}"
    );
}
