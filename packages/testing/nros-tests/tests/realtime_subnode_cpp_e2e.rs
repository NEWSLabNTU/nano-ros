//! Phase 273 W4 — runtime E2E for the sub-node 2-group C++ workspace
//! (`realtime-cpp`, nros↔nros, no ROS 2 required).
//!
//! **What this proves (RFC-0047 core capability):**
//! `examples/workspaces/realtime-cpp` declares ONE C++ component:
//! `subnode_pkg::SubNode`, a `nros::NodeWithTimers<2>` subclass that creates TWO
//! callback groups in its constructor:
//!
//!   ```cpp
//!   auto ctrl_grp  = create_callback_group("ctrl");
//!   auto telem_grp = create_callback_group("telem");
//!   create_timer_in_group<SubNode, &SubNode::on_ctrl>(ctrl_grp, 10);   // 10 ms
//!   create_timer_in_group<SubNode, &SubNode::on_telem>(telem_grp, 100); // 100 ms
//!   ```
//!
//! `system.toml [[component]].group_tasks = { ctrl = "high", telem = "low" }` maps
//! the two groups to the two sched tasks. The codegen emits BEFORE construction:
//!
//!   ```cpp
//!   nros_cpp_bind_group_sched(exec, "sub_node", "/", "ctrl",  SC_HIGH);
//!   nros_cpp_bind_group_sched(exec, "sub_node", "/", "telem", SC_LOW);
//!   ```
//!
//! This is the capability the per-node-name table (phase-272) CANNOT express: two
//! callbacks of ONE node scheduled on distinct sched tasks. This test verifies it runs.
//!
//! The E2E asserts: the high-sched-task `/ctrl` (10 ms) publishes ≥3× more messages than
//! the low-sched-task `/telem` (100 ms) in the same observation window, confirming both
//! groups of the same node are scheduled at their respective tier cadences.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_subnode_cpp_e2e`

use nros_tests::fixtures::{
    ManagedProcess, RequireFixture, ZenohRouter, build_native_workspace_cpp_subnode_realtime_entry,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Phase 273 W4 (RFC-0047) — ONE node's two callback groups schedule on two sched tasks.
///
/// `subnode_pkg::SubNode` creates both a "ctrl" group (10 ms) and a "telem" group
/// (100 ms) in code. `system.toml group_tasks` maps them to the "high" and "low"
/// sched tasks. The entry seeds `bind_group_sched` for both groups before construction,
/// wiring each timer to its sched context.
///
/// Acceptance: `/ctrl` (high-sched-task, 10 ms) publishes ≥3× more messages than `/telem`
/// (low-sched-task, 100 ms) in the same observation window. This confirms per-group binding
/// — both callbacks of ONE node run at their declared cadences on distinct tiers.
#[rstest]
fn realtime_subnode_cpp_two_groups_on_two_sched_tasks(zenohd_unique: ZenohRouter) {
    require_zenohd();
    let entry = build_native_workspace_cpp_subnode_realtime_entry()
        .map(|p| p.to_path_buf())
        .require("realtime-cpp entry");
    let locator = zenohd_unique.locator();

    let mut ctrl = nros_tests::fixtures::spawn_int32_sink(Some("/ctrl"), &locator);
    let mut telem = nros_tests::fixtures::spawn_int32_sink(Some("/telem"), &locator);

    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "12000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "5");
    let mut proc = ManagedProcess::spawn_image(cmd, "realtime-cpp-subnode")
        .expect("spawn realtime-cpp-subnode entry");

    // Anchor on the SLOW sched task: once telem (100 ms) has published 5 times, enough
    // wall time (~0.5 s+) has elapsed for the 10 ms ctrl sched task to have published
    // many more — proving both groups of ONE node are live on distinct sched tasks.
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
                "low-sched-task /telem group never reached 5 publishes — \
                 the 'telem' callback group was not scheduled on its sched task"
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
                "high-sched-task /ctrl group produced nothing — \
                 the 'ctrl' callback group was not scheduled on its sched task"
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
        "expected ≥5 low-sched-task /telem publishes, got {telem_n}"
    );
    // 10 ms vs 100 ms ⇒ ~10×; assert a clear ≥3× margin to stay robust against
    // native timer jitter and zenoh delivery batching. This is the RFC-0047 proof:
    // ONE node's two callback groups scheduled on two distinct sched tasks.
    assert!(
        ctrl_n >= telem_n * 3,
        "expected the high-sched-task 'ctrl' group (/ctrl, 10 ms) to publish ≥3× \
         the low-sched-task 'telem' group (/telem, 100 ms): ctrl={ctrl_n} telem={telem_n} \
         — per-group binding may not be seeding bind_group_sched correctly (RFC-0047)"
    );

    eprintln!(
        "[realtime_subnode_cpp_e2e] PASS: ctrl={ctrl_n} telem={telem_n} \
         ratio={:.1}× (≥3× required) — one node's two groups on two sched tasks",
        ctrl_n as f64 / telem_n.max(1) as f64
    );
}
