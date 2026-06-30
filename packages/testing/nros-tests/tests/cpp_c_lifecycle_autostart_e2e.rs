//! Phase 269 W2 — E2E for C/C++ entry lifecycle autostart (`[lifecycle] autostart = "active"`).
//!
//! The `ws-lifecycle-c` and `ws-lifecycle-cpp` workspace entries each boot a single talker node
//! whose bringup declares `[lifecycle] autostart = "active"`. The generated `__nros_entry_setup`
//! (emit_c.rs / emit_cpp.rs W2) calls `nros_cpp_lifecycle_autostart(executor, 2u)` — this
//! registers the 5 REP-2002 lifecycle services and drives the boot transitions
//! Unconfigured→Configured→Active with no external intervention.
//!
//! This test asserts the observable outcome: `ros2 lifecycle get /talker` reports `active`
//! on its own, with no manual `ros2 lifecycle set`.
//!
//! Requires ROS 2 + `rmw_zenoh_cpp` (overlay via `just rmw_zenoh setup`, or a distro install).
//! Skips (does not fail) when absent — same contract as other ROS 2 interop tests.
//!
//! Run with:
//! ```
//! cargo nextest run -p nros-tests --test cpp_c_lifecycle_autostart_e2e
//! ```

use nros_tests::{
    fixtures::{
        ZenohRouter, build_native_workspace_c_lifecycle_entry,
        build_native_workspace_cpp_lifecycle_entry,
    },
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

/// Poll `ros2 <subcommand>` until its output contains `marker` (case-insensitive) or timeout.
fn poll_ros2_until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let marker_lc = marker.to_lowercase();
    let mut last = String::new();
    while Instant::now() < deadline {
        last = run_ros2(locator, subcommand);
        if last.to_lowercase().contains(&marker_lc) {
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

/// Spawn the entry binary with the standard set of env vars.
fn spawn_entry(binary: std::path::PathBuf, label: &str, locator: &str) -> ManagedProcess {
    let mut cmd = Command::new(binary);
    cmd.env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("RUST_LOG", "info")
        .env("NROS_ENTRY_SPIN_MS", "30000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, label).expect("spawn entry")
}

/// W2 C — the generated `__nros_entry_setup` calls `nros_cpp_lifecycle_autostart(executor, 2u)`
/// so the C talker node reaches `active` at boot, observable via `ros2 lifecycle get /talker`.
#[rstest]
fn c_lifecycle_autostart_reaches_active() {
    if !require_ros2() {
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }
    let entry = build_native_workspace_c_lifecycle_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| skip!("ws-lifecycle-c entry fixture not built: {e}"));

    let port = pick_port();
    let router = ZenohRouter::start(port).expect("start zenohd");
    let locator = router.locator();

    let mut node = spawn_entry(entry, "c-lifecycle-entry", &locator);

    // Discover the managed node via the REP-2002 service surface.
    let nodes_out = poll_ros2_until(&locator, "lifecycle nodes", "/", Duration::from_secs(20));
    let lifecycle_node = first_lifecycle_node(&nodes_out).unwrap_or_else(|| {
        node.kill();
        panic!(
            "`ros2 lifecycle nodes` listed no managed node — \
             the C workspace entry's REP-2002 services are not on the wire:\n{nodes_out}"
        )
    });

    // Autostart must already have driven the node to active — no manual `set` issued.
    let state = poll_ros2_until(
        &locator,
        &format!("lifecycle get --no-daemon --spin-time 0.1 {lifecycle_node}"),
        "active",
        Duration::from_secs(20),
    );

    node.kill();

    assert!(
        state.to_lowercase().contains("active"),
        "expected the C autostart-managed node {lifecycle_node} to be `active` at boot, got:\n{state}"
    );
}

/// W2 C++ — the generated `__nros_entry_setup` calls `nros_cpp_lifecycle_autostart(__exec, 2u)`
/// (via `::nros::global_handle()`) so the C++ talker node reaches `active` at boot.
#[rstest]
fn cpp_lifecycle_autostart_reaches_active() {
    if !require_ros2() {
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }
    let entry = build_native_workspace_cpp_lifecycle_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| skip!("ws-lifecycle-cpp entry fixture not built: {e}"));

    let port = pick_port();
    let router = ZenohRouter::start(port).expect("start zenohd");
    let locator = router.locator();

    let mut node = spawn_entry(entry, "cpp-lifecycle-entry", &locator);

    // Discover the managed node.
    let nodes_out = poll_ros2_until(&locator, "lifecycle nodes", "/", Duration::from_secs(20));
    let lifecycle_node = first_lifecycle_node(&nodes_out).unwrap_or_else(|| {
        node.kill();
        panic!(
            "`ros2 lifecycle nodes` listed no managed node — \
             the C++ workspace entry's REP-2002 services are not on the wire:\n{nodes_out}"
        )
    });

    // Autostart drives it to active — assert with no external transition.
    let state = poll_ros2_until(
        &locator,
        &format!("lifecycle get --no-daemon --spin-time 0.1 {lifecycle_node}"),
        "active",
        Duration::from_secs(20),
    );

    node.kill();

    assert!(
        state.to_lowercase().contains("active"),
        "expected the C++ autostart-managed node {lifecycle_node} to be `active` at boot, got:\n{state}"
    );
}
