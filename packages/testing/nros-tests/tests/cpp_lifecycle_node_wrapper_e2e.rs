//! Phase 270 (#103) — runtime E2E for the C++ `nros::LifecycleNode` wrapper.
//!
//! `ws-lifecycle-cpp`'s `native_managed_entry` boots `ManagedTalker`, a managed node
//! written with the wrapper (NOT the phase-269 entry-autostart codegen — its
//! `managed_bringup` has no `[lifecycle]` block). In its install hook the node
//! `bind()`s the executor, `register_services()` (binding the `on_*` trampolines), and
//! `autostart(nros::LifecycleState::Active)` — driving Configure→Activate through the
//! wrapper so the rclcpp-shape overrides fire.
//!
//! The overrides print markers and publishing is gated on the Active state, so this
//! test asserts the observable proof that the wrapper works end to end:
//!   - `LC:on_configure` + `LC:on_activate` — the overrides ran (trampolines dispatch).
//!   - `LC:state=3` — `get_state()` reads Active (REP-2002 numbering) through the handle.
//!   - `Published:` — the timer publishes only after `on_activate` set the gate.
//!
//! Run with:
//! ```
//! cargo nextest run -p nros-tests --test cpp_lifecycle_node_wrapper_e2e
//! ```

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_cpp_lifecycle_managed_entry,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// The wrapper-managed node reaches Active on its own and its overrides fire.
#[rstest]
fn managed_node_wrapper_reaches_active_and_publishes(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let entry = build_native_workspace_cpp_lifecycle_managed_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("managed lifecycle entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "8000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut node =
        ManagedProcess::spawn_command(cmd, "managed-lifecycle").expect("spawn managed entry");

    // The gated `Published:` line only appears once on_activate flipped the gate, so
    // waiting for it proves the whole wrapper chain ran (register + configure + activate).
    let out = node
        .wait_for_output_count("Published:", 2, Duration::from_secs(12))
        .unwrap_or_else(|_| {
            node.kill();
            panic!(
                "managed node never published — the nros::LifecycleNode wrapper did not \
                 drive Configure→Activate (phase-270 / issue #103)"
            )
        });

    node.kill();

    for marker in ["LC:on_configure", "LC:on_activate", "LC:state=3"] {
        assert!(
            out.contains(marker),
            "expected wrapper marker {marker:?} in the managed node's output, got:\n{out}"
        );
    }
}
