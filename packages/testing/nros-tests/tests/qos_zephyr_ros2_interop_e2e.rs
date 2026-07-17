//! issue #141 — the PUB direction of the ROS 2 interop matrix: an nros
//! (zenoh-pico) publisher's samples must reach a stock `rmw_zenoh_cpp`
//! subscriber (`ros2 topic echo`).
//!
//! Before this test the direction had NO green coverage anywhere: the
//! `ros2-string-interop` fixture proves the opposite direction
//! (`demo_nodes_cpp talker` → nros sub), the zephyr capability e2es assert
//! via nros-native observers, and the #133-era interop tests soft-passed on
//! zero received. #141 was filed when `ros2 topic echo` appeared dead against
//! healthy image publishers; the keyexprs turned out identical
//! (`<domain>/<topic>/<type>/TypeHashNotSupported` both sides) and the
//! direction works — this test pins it so a real regression can't hide again.
//!
//! Reuses the phase-276 W5 `ws-qos-rust` Zephyr image (both nodes on-target;
//! `/qos_ok` republished at 1 Hz) and its baked router port — the
//! `zephyr-qos-port` nextest group serializes this with
//! the `entry_e2e` zephyr_rust_qos cell so the routers never collide.
//!
//! Requires ROS 2 + `rmw_zenoh_cpp` (skips when absent) and the west-lane
//! fixture (skips when `zephyr.exe` is absent).
//!
//! Run with: `cargo nextest run -p nros-tests --test qos_zephyr_ros2_interop_e2e`

use nros_tests::{
    alloc::port_of,
    fixtures::{ZenohRouter, ZephyrPlatform, ZephyrProcess, build_zephyr_workspace_rust_qos_entry},
    matrix::{Lang, PlatformId, Workload},
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
    skip,
};
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// The router port baked into the qos zephyr entry — the allocator's
/// (zephyr, rust, qos) number, matching the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR` bake.
const QOS_ZEPHYR_ENTRY_PORT: u16 = port_of(PlatformId::ZephyrNativeSim, Lang::Rust, Workload::Qos);

#[test]
fn nros_zephyr_publisher_reaches_ros2_topic_echo() {
    if !require_ros2() {
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }
    let entry = build_zephyr_workspace_rust_qos_entry()
        .unwrap_or_else(|e| skip!("zephyr qos workspace entry not built (west): {e}"));

    let router = ZenohRouter::start_on("127.0.0.1", QOS_ZEPHYR_ENTRY_PORT)
        .unwrap_or_else(|e| skip!("zenohd failed to start on {QOS_ZEPHYR_ENTRY_PORT}: {e}"));
    let locator = router.locator();

    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // Poll `ros2 topic echo --once` until a sample lands. Each attempt pays
    // multi-second ros2 CLI startup, so budget generously but bail on the
    // first success.
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, &locator);
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut last = String::new();
    let mut delivered = false;
    while Instant::now() < deadline {
        let script = format!(
            "{env} && timeout 12 ros2 topic echo --once /qos_ok std_msgs/msg/Int32 \
             --no-daemon --spin-time 2 2>&1"
        );
        let out = Command::new("bash")
            .args(["-c", &script])
            .output()
            .expect("failed to spawn bash for ros2 invocation");
        last = String::from_utf8_lossy(&out.stdout).into_owned();
        if last.contains("data:") {
            delivered = true;
            break;
        }
    }

    zephyr.kill();

    assert!(
        delivered,
        "`ros2 topic echo` (rmw_zenoh_cpp) never received the Zephyr entry's \
         1 Hz `/qos_ok` republish — the nros-pub → ros2-sub interop direction \
         regressed (issue #141).\nlast ros2 output:\n{last}"
    );
}
