//! Phase 211.A — the deploy "second-stage": a PLANNED native system, built
//! from a launch topology, actually RUNS and publishes to the ROS graph.
//!
//! `examples/workspaces/rust/src/native_entry` is an Entry pkg whose
//! `nros::main!(launch = "demo_bringup:system.launch.xml")` resolves the
//! `demo_bringup` topology (a `talker` + a `listener` node), bakes the typed
//! entry, and the native board runs it. This test proves the full
//! plan → codegen → build → boot → spin → **publish** pipeline end-to-end:
//! the deployed binary publishes `std_msgs/Int32` on `/chatter`, and a
//! separate subscriber process receives it.
//!
//! ## Why cross-process
//!
//! The deployed system hosts both nodes in ONE process on ONE zenoh session.
//! zenoh-pico (the `nros-rmw-zenoh` backend) has a documented "write filter"
//! limitation: **in-process pub/sub does not deliver** — regardless of whether
//! the endpoints share a session or run in distinct executors in the same OS
//! process (see `tests/trigger_conditions.rs` header +
//! `tests/component_runtime.rs` "Note on absent e2e pub→sub test"). So the
//! deployed system's own in-process listener never sees its talker; delivery
//! is only observable from a SEPARATE process. This test therefore runs the
//! deployed system as the publisher and a standalone `listener` binary as the
//! cross-process subscriber — the canonical out-of-process pub/sub topology
//! every other nano-ros pubsub e2e uses.
//!
//! The deployed binary is driven by the macro's hosted-spin knob
//! `NROS_ENTRY_SPIN_MS` (the env-gated bounded spin in
//! `nros-macros::main_macro`), so it spins + publishes for the test window
//! then exits cleanly.

use std::{process::Command, time::Duration};

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, ZenohRouter, build_native_listener, build_native_workspace_rust_entry,
        require_zenohd, zenohd_unique,
    },
};
use rstest::rstest;

/// The planned native deploy publishes to the ROS graph; a cross-process
/// subscriber receives. Proves 211.A's deploy second-stage.
#[rstest]
fn deployed_native_system_publishes_to_ros_graph(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    // Prebuilt fixtures (built by `just native build-fixtures` /
    // `build-workspace-fixtures`); tier-aware skip when absent.
    let entry = match build_native_workspace_rust_entry() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!("native_entry fixture not built: {e}"),
    };
    let listener_bin = build_native_listener().expect("build native listener");
    let locator = zenohd_unique.locator();

    // Cross-process subscriber first, so its subscription is declared before
    // the deployed talker starts publishing.
    let mut listener_cmd = Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("spawn listener");
    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(8))
        .expect("listener did not become ready");

    // The PLANNED deploy: spins for ~10 s (NROS_ENTRY_SPIN_MS) publishing
    // /chatter on its 1 Hz timer. Its own in-process listener sees nothing
    // (zenoh-pico in-process limitation) — that's expected; delivery is
    // asserted on the cross-process listener below.
    let mut entry_cmd = Command::new(&entry);
    entry_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "10000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut deploy =
        ManagedProcess::spawn_command(entry_cmd, "native_entry").expect("spawn native_entry");

    let listener_output = listener
        .wait_for_output_count("Received:", 1, Duration::from_secs(15))
        .expect("cross-process subscriber received nothing from the deployed system");

    deploy.kill();
    listener.kill();

    let received = count_pattern(&listener_output, "Received:");
    assert!(
        received >= 1,
        "deployed native system must publish to the ROS graph (cross-process Received = {received}):\n{listener_output}"
    );
}
