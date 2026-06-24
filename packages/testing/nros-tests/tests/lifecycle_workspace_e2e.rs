//! phase-263 A3 (Track D) — runtime E2E for the managed (lifecycle) workspace via the
//! REP-2002 service surface (`ros2 lifecycle …`).
//!
//! `examples/workspaces/ws-lifecycle-rust` declares `[lifecycle] autostart = "active"`;
//! the `native_entry` bakes `nros/lifecycle-services`, so `nros::main!` (phase-264 W2)
//! registers the 5 REP-2002 lifecycle services on the executor and drives the boot
//! **autostart** transitions Configure→Activate. Unlike the standalone
//! `ros2_lifecycle_interop` test (which sets transitions manually), this asserts the
//! workspace's distinguishing behaviour: the node reaches `active` **on its own** at
//! boot, with no external `ros2 lifecycle set`.
//!
//! Requires ROS 2 + `rmw_zenoh_cpp` (overlay via `just rmw_zenoh setup`, or a distro
//! install). Skips (does not fail) when absent — same contract as the other ROS 2
//! interop tests. The lifecycle node name is discovered from `ros2 lifecycle nodes`
//! rather than hard-coded, so the assertion is robust to the entry's executor node name.
//!
//! Run with: `cargo nextest run -p nros-tests --test lifecycle_workspace_e2e`

use nros_tests::{
    fixtures::{ZenohRouter, build_native_workspace_rust_lifecycle_entry},
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
    skip,
};
use rstest::rstest;
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// Run `ros2 <subcommand>` against `locator`; return combined stdout+stderr.
fn run_ros2(locator: &str, subcommand: &str) -> String {
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
    let script = format!("{env} && timeout 10 ros2 {subcommand} 2>&1");
    let out = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("failed to spawn bash for ros2 invocation");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Poll `ros2 <subcommand>` until its output contains `marker` (case-insensitive).
fn poll_ros2_until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let marker = marker.to_lowercase();
    let mut last = String::new();
    while Instant::now() < deadline {
        last = run_ros2(locator, subcommand);
        if last.to_lowercase().contains(&marker) {
            return last;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    last
}

/// First `/`-prefixed node name in `ros2 lifecycle nodes` output, if any.
fn first_lifecycle_node(nodes_out: &str) -> Option<String> {
    nodes_out
        .lines()
        .map(|l| l.trim())
        .find(|l| l.starts_with('/'))
        .map(|l| l.to_string())
}

fn pick_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local_addr")
        .port()
}

/// A3 — the managed workspace's autostart drives its node to `active` at boot,
/// observable over the REP-2002 service surface with no manual transition.
#[rstest]
fn lifecycle_workspace_autostart_reaches_active() {
    if !require_ros2() {
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }
    let entry = build_native_workspace_rust_lifecycle_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| skip!("lifecycle workspace entry fixture not built: {e}"));

    let port = pick_port();
    let router = ZenohRouter::start(port).expect("start zenohd");
    let locator = router.locator();

    let mut cmd = Command::new(&entry);
    cmd.env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("RUST_LOG", "info")
        .env("NROS_ENTRY_SPIN_MS", "30000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut node = ManagedProcess::spawn_command(cmd, "lifecycle-entry").expect("spawn entry");

    // Discover the managed node (the entry's executor node hosting the 5 services).
    let nodes_out = poll_ros2_until(&locator, "lifecycle nodes", "/", Duration::from_secs(20));
    let lifecycle_node = first_lifecycle_node(&nodes_out).unwrap_or_else(|| {
        node.kill();
        panic!("`ros2 lifecycle nodes` listed no managed node — the workspace entry's REP-2002 services are not on the wire:\n{nodes_out}")
    });

    // Autostart should already have driven it to active — no manual `set` issued.
    let state = poll_ros2_until(
        &locator,
        &format!("lifecycle get --no-daemon --spin-time 0.1 {lifecycle_node}"),
        "active",
        Duration::from_secs(20),
    );

    node.kill();

    assert!(
        state.to_lowercase().contains("active"),
        "expected the autostart-managed node {lifecycle_node} to be `active` at boot, got:\n{state}"
    );
}
