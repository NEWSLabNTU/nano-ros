//! phase-276 W3 (#102 H1, issue #128) — runtime E2E for LIFECYCLE on embedded:
//! the `ws-lifecycle-rust` workspace's Zephyr (native_sim) Entry, asserted over
//! the REP-2002 service surface (`ros2 lifecycle …`).
//!
//! Before #128 the `nros::main!` `Framework::Zephyr` emit arm carried only
//! register+spin — a `system.toml [lifecycle]` was silently ignored on Zephyr.
//! The fix gives the arm OwnedSpin parity: it now emits `apply_lifecycle(...)`
//! after the registers, installing the five REP-2002 lifecycle services and
//! driving the boot **autostart** (Configure → Activate) on the embedded
//! target — no external `ros2 lifecycle set`.
//!
//! Assertion mirrors the native `lifecycle_workspace_e2e`: discover the managed
//! node via `ros2 lifecycle nodes`, then poll `ros2 lifecycle get` until it
//! reports `active`. The `zephyr.exe` runs as a host process (native_sim, NSOS
//! host sockets) and dials the baked `tcp/127.0.0.1:17847` locator.
//!
//! Requires ROS 2 + `rmw_zenoh_cpp` (skips when absent) and the west-lane
//! fixture (`just zephyr build-fixtures`; skips when `zephyr.exe` is absent).
//!
//! Run with: `cargo nextest run -p nros-tests --test lifecycle_zephyr_entry_e2e`

use nros_tests::{
    fixtures::{
        ZenohRouter, ZephyrPlatform, ZephyrProcess, build_zephyr_workspace_rust_lifecycle_entry,
    },
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
    skip,
};
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// The router port baked into the lifecycle zephyr entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17847"`).
const LIFECYCLE_ZEPHYR_ENTRY_PORT: u16 = 17847;

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

#[test]
fn lifecycle_zephyr_entry_autostart_reaches_active() {
    if !require_ros2() {
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }
    let entry = build_zephyr_workspace_rust_lifecycle_entry()
        .unwrap_or_else(|e| skip!("zephyr lifecycle workspace entry not built (west): {e}"));

    // Router on the exact port the fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router = ZenohRouter::start_on("127.0.0.1", LIFECYCLE_ZEPHYR_ENTRY_PORT)
        .unwrap_or_else(|e| skip!("zenohd failed to start on {LIFECYCLE_ZEPHYR_ENTRY_PORT}: {e}"));
    let locator = router.locator();

    // Boot the Zephyr native_sim image (runs until killed).
    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // Discover the managed node (the entry's executor node hosting the 5 services).
    // `--no-daemon`: the env snippet stops the daemon (it holds its own zenoh
    // session), so a daemon-using invocation would respawn one per poll — each
    // poll then pays multi-second daemon startup and the 40 s budget (and
    // nextest's 60 s slow-timeout) drains on CLI churn instead of discovery.
    let nodes_out = poll_ros2_until(
        &locator,
        "lifecycle nodes --no-daemon --spin-time 2",
        "/",
        Duration::from_secs(40),
    );
    let lifecycle_node = first_lifecycle_node(&nodes_out).unwrap_or_else(|| {
        zephyr.kill();
        panic!(
            "`ros2 lifecycle nodes` listed no managed node — the Zephyr entry's \
             REP-2002 services are not on the wire (276 W3 / #128):\n{nodes_out}"
        )
    });

    // Autostart should already have driven it to active — no manual `set` issued.
    // `--spin-time 2` (not the native test's 0.1): the Zephyr native_sim
    // responder answers the get-state service noticeably slower through NSOS,
    // and a 0.1 s spin returns empty on every poll attempt.
    let state = poll_ros2_until(
        &locator,
        &format!("lifecycle get --no-daemon --spin-time 2 {lifecycle_node}"),
        "active",
        Duration::from_secs(40),
    );

    zephyr.kill();

    assert!(
        state.to_lowercase().contains("active"),
        "expected the autostart-managed Zephyr node {lifecycle_node} to be `active` \
         at boot, got:\n{state}"
    );
}
