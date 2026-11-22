//! Issue 2828 — a C lifecycle node, driven by stock `ros2 lifecycle`.
//!
//! `examples/linux/c/lifecycle-node` binds one node (`rclc_executor_node_init`)
//! and registers the five REP-2002 services through the executor-level call
//! (`rclc_executor_register_lifecycle_services`). Issue 2429's fix serves them
//! on that node. The `nros-node` unit tests hold the names; this holds what a
//! stock client sees:
//!
//! 1. `ros2 lifecycle nodes` lists exactly `/c_lifecycle_node`: no
//!    `//get_state` machine and no phantom node.
//! 2. `ros2 node list` shows `/c_lifecycle_node` and nothing nameless.
//! 3. `get` answers `unconfigured`; `set configure` and `set activate`
//!    succeed, each runs the example's own callback, and `get` follows.
//! 4. `list` names the transitions open from `inactive`.
//! 5. `deactivate` and `cleanup` close the cycle back to `unconfigured`.
//!
//! Build ahead of time with `just linux build-fixtures`; nothing compiles at
//! run time.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, build_c_lifecycle_node, require_ros2,
        require_zenohd, ros2_env_setup_with_locator, zenohd_unique,
    },
    ros2::DEFAULT_ROS_DISTRO,
};
use rstest::rstest;
use std::{
    process::Command,
    time::{Duration, Instant},
};

const NODE: &str = "/c_lifecycle_node";

fn start_node(locator: &str) -> ManagedProcess {
    let binary = build_c_lifecycle_node().require("c-lifecycle-node");
    let mut cmd = Command::new(binary);
    cmd.env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        // Long enough for every query below; the process is killed on drop.
        .env("NROS_SPIN_MS", "120000");
    let mut node =
        ManagedProcess::spawn_image(cmd, "c-lifecycle-node").expect("spawn c-lifecycle-node");
    let boot = node
        .wait_for_output_pattern("Ready. Drive it", Duration::from_secs(15))
        .expect("c-lifecycle-node never reached its ready line");
    assert!(
        boot.contains("Lifecycle services registered on /c_lifecycle_node"),
        "boot log lacks the registration line:\n{boot}"
    );
    node
}

/// `ros2 <subcommand>` against `locator`, stdout and stderr together.
fn ros2(locator: &str, subcommand: &str) -> String {
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
    let deadline = nros_tests::process::deadline(10);
    let script = format!("{env} && {deadline} ros2 {subcommand} --no-daemon 2>&1");
    let out = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("spawn bash for ros2");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Poll `ros2 <subcommand>` until its output holds `marker` (case-insensitive):
/// discovery through the router is not instantaneous.
fn until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let end = Instant::now() + timeout;
    let marker = marker.to_lowercase();
    let mut last = String::new();
    while Instant::now() < end {
        last = ros2(locator, subcommand);
        if last.to_lowercase().contains(&marker) {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    last
}

/// The `/…` lines of a `ros2 … list`-shaped output.
fn names(out: &str) -> Vec<&str> {
    out.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/'))
        .collect()
}

fn set_runs_callback(node: &mut ManagedProcess, locator: &str, transition: &str, callback: &str) {
    let out = ros2(
        locator,
        &format!("lifecycle set --spin-time 0.1 {NODE} {transition}"),
    );
    assert!(
        out.contains("Transitioning successful"),
        "`ros2 lifecycle set {NODE} {transition}` did not succeed:\n{out}"
    );
    node.wait_for_output_pattern(callback, Duration::from_secs(5))
        .unwrap_or_else(|e| panic!("`{transition}` never ran the example's {callback}: {e}"));
}

/// `get` reads exactly `state` (`active` must not match `inactive`).
fn assert_state(locator: &str, state: &str) {
    let out = until(
        locator,
        &format!("lifecycle get --spin-time 0.1 {NODE}"),
        state,
        Duration::from_secs(10),
    );
    let first = out.trim().to_lowercase();
    assert!(
        first.starts_with(state),
        "`ros2 lifecycle get {NODE}` should read `{state}`, got:\n{out}"
    );
}

#[rstest]
fn rclc_lifecycle_node_answers_ros2_lifecycle(zenohd_unique: ZenohRouter) {
    require_zenohd();
    require_ros2();
    let locator = zenohd_unique.locator();
    let mut node = start_node(&locator);

    // 1. exactly the node, under its own name.
    let nodes = until(
        &locator,
        "lifecycle nodes --spin-time 0.1",
        NODE,
        Duration::from_secs(15),
    );
    assert_eq!(
        names(&nodes),
        [NODE],
        "`ros2 lifecycle nodes` must list exactly {NODE}:\n{nodes}"
    );

    // 2. no nameless or phantom node beside it.
    let graph = until(&locator, "node list", NODE, Duration::from_secs(10));
    assert_eq!(
        names(&graph),
        [NODE],
        "`ros2 node list` must show only {NODE}:\n{graph}"
    );

    // 3. the cycle up, each transition running the example's own callback.
    assert_state(&locator, "unconfigured");
    set_runs_callback(&mut node, &locator, "configure", "on_configure");
    assert_state(&locator, "inactive");

    // 4. what is open from `inactive`.
    let list = ros2(&locator, &format!("lifecycle list --spin-time 0.1 {NODE}"));
    for t in ["activate", "cleanup", "shutdown"] {
        assert!(
            list.contains(t),
            "`ros2 lifecycle list {NODE}` lacks `{t}`:\n{list}"
        );
    }

    set_runs_callback(&mut node, &locator, "activate", "on_activate");
    assert_state(&locator, "active");

    // 5. and back down to `unconfigured`.
    set_runs_callback(&mut node, &locator, "deactivate", "on_deactivate");
    assert_state(&locator, "inactive");
    set_runs_callback(&mut node, &locator, "cleanup", "on_cleanup");
    assert_state(&locator, "unconfigured");
}

// Bound to `interop::CELLS`; drift turns this red.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound("c_lifecycle_interop", &[(Linux, C, Zenoh, Lifecycle)]);
}
