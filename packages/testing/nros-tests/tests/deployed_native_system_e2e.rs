//! Phase 211.A — the deploy "second-stage": a PLANNED native system, built
//! from a launch topology, actually RUNS and publishes to the ROS graph.
//!
//! `examples/workspaces/rust/src/linux_entry` is an Entry pkg whose
//! `nros::main!(launch = "demo_bringup:system.launch.xml")` resolves the
//! `demo_bringup` topology (a `talker` + a `listener` node), bakes the typed
//! entry, and the native board runs it. This test proves the full
//! plan → codegen → build → boot → spin → **publish** pipeline end-to-end:
//! the deployed binary publishes `std_msgs/String` on `/chatter` (the ROS 2
//! tutorial talker since phase-535), and a separate subscriber process — the
//! standalone tutorial listener — receives it.
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
        ManagedProcess, RequireFixture, ZenohRouter, build_native_listener,
        build_native_workspace_rust_entry, require_zenohd, zenohd_unique,
    },
};
use rstest::rstest;

/// The planned native deploy publishes to the ROS graph; a cross-process
/// subscriber receives. Proves 211.A's deploy second-stage.
#[rstest]
fn deployed_native_system_publishes_to_ros_graph(zenohd_unique: ZenohRouter) {
    require_zenohd();

    // Prebuilt fixtures (built by `just linux build-fixtures` /
    // `build-workspace-fixtures`); out of lane skips, absent fails.
    let entry = build_native_workspace_rust_entry()
        .require("linux_entry")
        .to_path_buf();
    let listener_bin = build_native_listener().require("build native listener");
    let locator = zenohd_unique.locator();

    // Cross-process subscriber first, so its subscription is declared before
    // the deployed talker starts publishing.
    let mut listener_cmd = Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut listener =
        ManagedProcess::spawn_image(listener_cmd, "listener").expect("spawn listener");
    listener
        .wait_for_output_pattern(
            nros_tests::output::LISTENER_READY_MARKER,
            Duration::from_secs(8),
        )
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
        ManagedProcess::spawn_image(entry_cmd, "linux_entry").expect("spawn linux_entry");

    let listener_output = listener
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            1,
            Duration::from_secs(15),
        )
        .expect("cross-process subscriber received nothing from the deployed system");

    deploy.kill();
    listener.kill();

    let received = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    assert!(
        received >= 1,
        "deployed native system must publish to the ROS graph (cross-process Received = {received}):\n{listener_output}"
    );
}

/// RFC-0051 §7c (phase-510 W3) — the cases this file runs are exactly the
/// Runtime cells `matrix::consumer_of` assigns to it. Fixture-free: a cell
/// added, retiered or reassigned without this list moving is a red here.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::DeployedNativeSystem,
        &[(P::Linux, L::Rust, R::Zenoh, W::EntryPubsub, K::Workspace)],
    );
}
