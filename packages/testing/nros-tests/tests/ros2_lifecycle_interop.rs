//! ROS 2 lifecycle interop — drives an nros lifecycle-node through the
//! REP-2002 service surface via `ros2 lifecycle …`.
//!
//! Requires:
//! - ROS 2 Humble installed under `/opt/ros/humble`
//! - `rmw_zenoh_cpp` overlay built by `just rmw_zenoh setup`
//!   (test falls back to a distro install if present)
//! - `zenohd` binary at `build/zenohd/zenohd`
//!
//! Skips (not fails) when any of the above is missing, matching the
//! existing ROS 2 interop test contract.

use nros_tests::{
    fixtures::{ZenohRouter, lifecycle_node_binary},
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
    skip,
};
use rstest::rstest;
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

/// Run `ros2 <subcommand>` against the given zenoh locator and return combined stdout+stderr.
///
/// Sources the pinned rmw_zenoh overlay via [`ros2_env_setup_with_locator`]
/// and wraps the subcommand in a 10s timeout so a hung query can't stall the
/// whole test. Lifecycle commands pass `--no-daemon` below so they use this
/// process' Zenoh session config instead of a daemon bound to a different
/// locator. Keep `--spin-time` short: ROS 2 Humble's lifecycle CLI can report
/// an invalid wait set after a long no-daemon spin even when the service
/// already replied.
fn run_ros2(locator: &str, subcommand: &str) -> String {
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
    let script = format!("{env} && timeout 10 ros2 {subcommand} 2>&1");
    let out = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("failed to spawn bash for ros2 invocation");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn poll_ros2_until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut last_output = String::new();
    let marker = marker.to_lowercase();

    while Instant::now() < deadline {
        last_output = run_ros2(locator, subcommand);
        if last_output.to_lowercase().contains(&marker) {
            return last_output;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    last_output
}

/// Get the next open TCP port on loopback; used to spawn a fresh zenohd.
fn pick_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local_addr")
        .port()
}

#[rstest]
fn ros2_lifecycle_full_cycle(lifecycle_node_binary: PathBuf) {
    if !require_ros2() {
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }

    // Fresh zenohd on a unique port so parallel interop tests don't clash.
    let port = pick_port();
    let router = ZenohRouter::start(port).expect("start zenohd");
    let locator = router.locator();
    eprintln!("zenohd listening on {locator}");

    // Start the nros lifecycle node.
    let mut cmd = Command::new(&lifecycle_node_binary);
    cmd.env("NROS_LOCATOR", &locator).env("RUST_LOG", "info");
    let mut node =
        ManagedProcess::spawn_command(cmd, "lifecycle-node").expect("spawn lifecycle-node");

    // Wait for the node to register its lifecycle services and print the
    // "Ready." line.
    let boot_log = node
        .wait_for_output_pattern("Ready. Drive the lifecycle", Duration::from_secs(15))
        .expect("lifecycle-node never reached ready state");
    assert!(
        boot_log.contains("Lifecycle services registered"),
        "boot log missing service-registration marker: {boot_log}"
    );

    // ── Assertion A: `ros2 lifecycle nodes` discovers /lifecycle_demo
    let nodes = poll_ros2_until(
        &locator,
        "lifecycle nodes --no-daemon --spin-time 0.1",
        "/lifecycle_demo",
        Duration::from_secs(10),
    );
    eprintln!("--- ros2 lifecycle nodes ---\n{nodes}");
    assert!(
        nodes.contains("/lifecycle_demo"),
        "ros2 lifecycle nodes did not list /lifecycle_demo:\n{nodes}"
    );

    // ── Assertion B: initial get returns unconfigured
    let state_before = run_ros2(
        &locator,
        "lifecycle get --no-daemon --spin-time 0.1 /lifecycle_demo",
    );
    eprintln!("--- ros2 lifecycle get (before) ---\n{state_before}");
    assert!(
        state_before.to_lowercase().contains("unconfigured"),
        "expected Unconfigured before configure, got:\n{state_before}"
    );

    // ── Assertion C: set configure transitions to inactive + fires on_configure
    let configure_out = run_ros2(
        &locator,
        "lifecycle set --no-daemon --spin-time 0.1 /lifecycle_demo configure",
    );
    eprintln!("--- ros2 lifecycle set configure ---\n{configure_out}");
    assert!(
        configure_out.contains("Transitioning successful"),
        "configure did not report success:\n{configure_out}"
    );

    // The on_configure callback should have logged to the node's stdout.
    let callback_log = node
        .wait_for_output_pattern("on_configure", Duration::from_secs(3))
        .expect("on_configure never logged");
    assert!(
        callback_log.contains("on_configure"),
        "on_configure callback marker missing from node stdout"
    );

    let state_after = poll_ros2_until(
        &locator,
        "lifecycle get --no-daemon --spin-time 0.1 /lifecycle_demo",
        "inactive",
        Duration::from_secs(5),
    );
    eprintln!("--- ros2 lifecycle get (after configure) ---\n{state_after}");
    assert!(
        state_after.to_lowercase().contains("inactive"),
        "expected Inactive after configure, got:\n{state_after}"
    );

    // ── Assertion D: list shows reachable transitions from Inactive
    let list_out = run_ros2(
        &locator,
        "lifecycle list --no-daemon --spin-time 0.1 /lifecycle_demo",
    );
    eprintln!("--- ros2 lifecycle list ---\n{list_out}");
    for marker in ["activate", "cleanup", "shutdown"] {
        assert!(
            list_out.contains(marker),
            "ros2 lifecycle list missing `{marker}`:\n{list_out}"
        );
    }

    // Explicit kill so the ManagedProcess Drop has nothing to race against.
    node.kill();
    drop(router);
}
