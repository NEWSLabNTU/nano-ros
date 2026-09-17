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

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter,
        build_native_workspace_cpp_lifecycle_managed_entry, require_ros2, require_zenohd,
        zenohd_unique,
    },
    ros2::DEFAULT_ROS_DISTRO,
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
        .require("managed lifecycle entry");
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
        .wait_for_output_count(
            nros_tests::output::INT32_TALKER_LOG_PREFIX,
            2,
            Duration::from_secs(12),
        )
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

/// phase-417 W4.f — a parameter declared THROUGH the lifecycle node is the one
/// `ros2 param get` reads.
///
/// This is the acceptance the wave is about, and it is a WIRE question rather
/// than a local one on purpose. `ManagedTalker::configure` calls
/// `declare_parameter<int64_t>("publish_period_ms", 200)` on `nros::LifecycleNode`,
/// not on the `rclcpp::Node` it binds, and prints what it got back. A wrapper
/// holding a parameter table of its own would print exactly the same line and
/// tell a remote peer nothing — that is the shape phase-426 spent six work items
/// removing, and printing the local read would not catch it coming back.
///
/// So the assertion is the peer's: the forwarder reaches the executor's one
/// `nros_params::ParameterServer`, which is the store the six `rcl_interfaces`
/// servers answer from, or `ros2 param get` finds nothing.
///
/// Live peer (`ros2` + `rmw_zenoh_cpp`) over the same ephemeral `zenohd` the
/// test above uses, so a host without ROS 2 SKIPS rather than reporting a pass.
#[rstest]
fn a_parameter_declared_through_the_lifecycle_node_reaches_ros2(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }
    let entry = build_native_workspace_cpp_lifecycle_managed_entry()
        .map(|p| p.to_path_buf())
        .require("managed lifecycle entry");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "30000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut node = ManagedProcess::spawn_command(cmd, "managed-lifecycle-params")
        .expect("spawn managed entry");

    // The node's OWN report first: it declared the parameter and read back the
    // value in effect. If this line is missing the wire assertion below would
    // fail for a reason that has nothing to do with the store.
    let boot = node
        .wait_for_output_pattern("LC:param publish_period_ms=", Duration::from_secs(15))
        .unwrap_or_else(|_| {
            node.kill();
            panic!(
                "the managed node never reported its declared parameter — \
                 `nros::LifecycleNode::declare_parameter` did not return (phase-417 W4.f)"
            )
        });
    assert!(
        boot.contains("LC:param publish_period_ms=200"),
        "the lifecycle node read back a value it did not declare, got:\n{boot}"
    );

    // And now the peer's. Retry for discovery, as every parameter cell here
    // does: the six `rcl_interfaces` servers have to be seen before they answer.
    let mut seen = String::new();
    for attempt in 1..=5 {
        seen = nros_tests::ros2::ros2_param_get(
            "/managed_talker",
            "publish_period_ms",
            &locator,
            DEFAULT_ROS_DISTRO,
        )
        .expect("failed to run ros2 param get");
        if seen.contains("Integer value is: 200") {
            break;
        }
        if attempt < 5 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    node.kill();

    assert!(
        seen.contains("Integer value is: 200"),
        "`ros2 param get /managed_talker publish_period_ms` did not see the value the \
         LIFECYCLE node declared. The node printed it, so the local read works and the \
         wire read does not — which is a SECOND parameter store on the wrapper, the \
         defect phase-426 removed (phase-417 W4.f). Output:\n{seen}"
    );
}
