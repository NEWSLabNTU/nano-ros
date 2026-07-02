//! Phase 272 W3 — runtime E2E for the rclcpp-shape 2-tier sched-context C++
//! workspace (`ws-realtime-cpp-rclcpp`, nros↔nros, no ROS 2 required).
//!
//! **What this proves (issue #124 dissolved):** `examples/workspaces/ws-realtime-cpp-rclcpp`
//! declares two **rclcpp-shape** (`SHAPE rclcpp`, IS-A-node) C++ components:
//! `ctrl_pkg::Ctrl` and `telem_pkg::Telem`, each a `nros::ComponentNode` subclass
//! whose ctor receives a `NodeHandle` and creates publishers / timers there.
//! `system.toml` maps `ctrl_node → [tiers.high]` (10 ms, posix pri 80) and
//! `telem_node → [tiers.low]` (100 ms, posix pri 10) via `[[node_overrides]]`.
//!
//! Issue #124 noted that rclcpp-shape nodes were NOT bound to their tier because
//! the entry only passed a bare `NodeHandle` (no sched-context field) — the tier
//! binding was configure-shape-only. Phase 272 W1+W2 dissolved this by moving
//! binding to a config-seeded `node_name → sched_context` table looked up at
//! `Executor::node_builder(name)`, the single site EVERY node funnels through
//! (including rclcpp via `ComponentNode` → `Node::create` → `nros_cpp_node_create`
//! → `node_builder`). The W2 codegen emits `nros_cpp_bind_node_name_sched` seeds
//! for ALL tiered nodes (including rclcpp-shape) BEFORE any construction.
//!
//! This test builds and runs the rclcpp-shape entry, subscribes to both `/ctrl`
//! and `/telem`, and asserts the high-tier `/ctrl` publishes at roughly 10× the
//! low-tier `/telem` rate (≥3× conservative margin) — proving the rclcpp-shape
//! node schedules on its configured tier, the #124 acceptance criterion.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_cpp_rclcpp_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink,
    build_native_workspace_cpp_rclcpp_realtime_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn an nros subscriber on `topic` (prints `Received: <n>` per message).
fn spawn_listener(topic: &'static str, locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
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

/// Phase 272 W3 (RFC-0047, issue #124) — the rclcpp-shape 2-tier entry schedules
/// both tiers at their declared cadences. Both `ctrl_pkg::Ctrl` and `telem_pkg::Telem`
/// are IS-A-node `ComponentNode` subclasses; their tier binding is resolved via the
/// W1+W2 `node_name → sched_context` table (no NodeHandle change). The 10 ms
/// high-tier `/ctrl` must publish ≥3× more messages than the 100 ms low-tier `/telem`
/// in the same observation window — confirming rclcpp-shape nodes land on their tier.
#[rstest]
fn realtime_tiers_cpp_rclcpp_schedule_high_and_low(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_cpp_rclcpp_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| {
            nros_tests::skip!("ws-realtime-cpp-rclcpp entry fixture not built: {e}")
        });
    let locator = zenohd_unique.locator();

    let mut ctrl = spawn_listener("/ctrl", &locator);
    let mut telem = spawn_listener("/telem", &locator);

    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "12000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "5");
    let mut proc = ManagedProcess::spawn_command(cmd, "realtime-cpp-rclcpp")
        .expect("spawn realtime-cpp-rclcpp entry");

    // Anchor on the SLOW tier: once telem (100 ms) has published 5 times, enough
    // wall time (~0.5 s+) has elapsed that the 10 ms ctrl tier should have published
    // many more — proving both tiers are live and the high tier runs faster.
    let telem_out = telem
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            5,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            proc.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "low-tier /telem never reached 5 publishes — \
                 rclcpp-shape telem_node was not scheduled on its tier"
            )
        });
    let ctrl_out = ctrl
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            1,
            Duration::from_secs(2),
        )
        .unwrap_or_else(|_| {
            proc.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "high-tier /ctrl produced nothing — \
                 rclcpp-shape ctrl_node was not scheduled on its tier"
            )
        });

    proc.kill();
    ctrl.kill();
    telem.kill();

    let telem_n =
        nros_tests::count_pattern(&telem_out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    let ctrl_n =
        nros_tests::count_pattern(&ctrl_out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);

    assert!(
        telem_n >= 5,
        "expected ≥5 low-tier /telem publishes, got {telem_n}"
    );
    // 10 ms vs 100 ms ⇒ ~10×; assert a clear ≥3× margin to stay robust against
    // native timer jitter and zenoh delivery batching. This is the #124 proof:
    // the rclcpp-shape ctrl_node scheduled on the high tier (not the default context).
    assert!(
        ctrl_n >= telem_n * 3,
        "expected the high tier (/ctrl rclcpp-shape, 10 ms) to publish ≥3× \
         the low tier (/telem rclcpp-shape, 100 ms): ctrl={ctrl_n} telem={telem_n} \
         — rclcpp-shape nodes may not have received their sched-context (issue #124)"
    );
}
