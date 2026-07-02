//! Phase 273 W4 — portability E2E for the sub-node 2-group C++ workspace
//! (`ws-realtime-cpp-subnode-portable`, RFC-0047 portability proof).
//!
//! **What this proves (RFC-0047 portability):**
//! The `subnode_pkg::SubNode` component is IDENTICAL to the sub-node fixture
//! (`ws-realtime-cpp-subnode`) but this workspace uses DIFFERENT tier names:
//!
//!   - `[tiers.fast]` (10 ms, priority 80)  instead of `[tiers.high]`
//!   - `[tiers.bulk]` (100 ms, priority 10) instead of `[tiers.low]`
//!
//! `deploy_bringup/system.toml group_tiers = { ctrl = "fast", telem = "bulk" }` — no
//! change to the package source. This proves the RFC-0047 coupling is removed: a
//! group-using component package can be deployed with any tier names by changing only
//! `system.toml`. The entry emits:
//!
//!   ```cpp
//!   nros_cpp_bind_group_sched(exec, "sub_node", "/", "ctrl",  SC_FAST);
//!   nros_cpp_bind_group_sched(exec, "sub_node", "/", "telem", SC_BULK);
//!   ```
//!
//! The E2E asserts both topics are live at the expected cadence ratio — proving
//! the portably-deployed package binds correctly to the renamed tiers.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_subnode_cpp_portable_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener,
    build_native_workspace_cpp_subnode_portable_entry, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn an nros subscriber on `topic`.
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

/// Phase 273 W4 (RFC-0047 portability) — sub-node package portable across workspace
/// tier naming. The component package (`subnode_pkg::SubNode`) is unchanged; only
/// the workspace `deploy_bringup/system.toml` uses different tier names ("fast"/"bulk"
/// vs "high"/"low"). Both groups still bind correctly and schedule at their cadences.
#[rstest]
fn realtime_subnode_cpp_portable_two_groups_bind_renamed_tiers(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_cpp_subnode_portable_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| {
            nros_tests::skip!("ws-realtime-cpp-subnode-portable entry fixture not built: {e}")
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
    let mut proc = ManagedProcess::spawn_command(cmd, "realtime-cpp-subnode-portable")
        .expect("spawn realtime-cpp-subnode-portable entry");

    // Wait for the slow tier (bulk/100 ms) to publish 5 times.
    let telem_out = telem
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            5,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            proc.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "portable workspace: /telem (bulk tier, 100 ms) never reached 5 publishes — \
                 group 'telem' may not have bound to the renamed 'bulk' tier"
            )
        });
    let ctrl_out = ctrl
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            1,
            Duration::from_secs(2),
        )
        .unwrap_or_else(|_| {
            proc.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "portable workspace: /ctrl (fast tier, 10 ms) produced nothing — \
                 group 'ctrl' may not have bound to the renamed 'fast' tier"
            )
        });

    proc.kill();
    ctrl.kill();
    telem.kill();

    let telem_n = nros_tests::count_pattern(&telem_out, nros_tests::output::LISTENER_LOG_PREFIX);
    let ctrl_n = nros_tests::count_pattern(&ctrl_out, nros_tests::output::LISTENER_LOG_PREFIX);

    assert!(
        telem_n >= 5,
        "expected ≥5 bulk-tier /telem publishes, got {telem_n}"
    );
    assert!(
        ctrl_n >= telem_n * 3,
        "expected the fast-tier 'ctrl' group (/ctrl, 10 ms) to publish ≥3× \
         the bulk-tier 'telem' group (/telem, 100 ms): ctrl={ctrl_n} telem={telem_n} \
         — portable workspace group binding may be broken (RFC-0047)"
    );

    eprintln!(
        "[realtime_subnode_cpp_portable_e2e] PASS: ctrl={ctrl_n} telem={telem_n} \
         ratio={:.1}× (≥3× required) — portable deployment to fast/bulk tiers",
        ctrl_n as f64 / telem_n.max(1) as f64
    );
}
