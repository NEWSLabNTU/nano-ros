//! Phase 211 acceptance — nano-ros ↔ a REAL upstream ROS 2 workspace.
//!
//! The synthetic `demo_bringup` fixtures prove the nano-ros plan→codegen→build→
//! boot→publish pipeline against nano-ros's OWN nodes. This test closes the
//! "behaves like real ROS production" gap from the other side: an UNMODIFIED
//! stock `demo_nodes_cpp talker` (rclcpp, `std_msgs/String` "Hello World: N" on
//! `/chatter`) publishes to the ROS graph over `rmw_zenoh_cpp`, and a nano-ros
//! node receives it — cross-vendor interop through a shared `zenohd`.
//!
//! (nano-ros can PARSE a real upstream launch — `play_launch_parser` handles
//! `demo_nodes_cpp/launch/topics/talker_listener.launch.xml` — but it cannot
//! PLAN/bake foreign rclcpp nodes, which lack nano-ros source metadata. So the
//! real-ROS proof is runtime interop, not baking someone else's nodes.)
//!
//! Gated on `ros2` + `rmw_zenoh_cpp` (skips cleanly when absent, e.g. minimal
//! CI lanes), and on the prebuilt nano-ros subscriber fixture.

use std::time::Duration;

use nros_tests::fixtures::{
    DEFAULT_ROS_DISTRO, ManagedProcess, Ros2Process, ZenohRouter, build_ros2_string_interop,
    is_rmw_zenoh_available, is_ros2_available, require_zenohd, zenohd_unique,
};
use rstest::rstest;

/// A stock `demo_nodes_cpp talker` reaches a nano-ros subscriber on `/chatter`.
#[rstest]
fn nros_subscriber_receives_stock_demo_nodes_cpp_talker(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_ros2_available() {
        nros_tests::skip!("ROS 2 not available");
    }
    if !is_rmw_zenoh_available() {
        nros_tests::skip!("rmw_zenoh_cpp not available");
    }
    let sub_bin = match build_ros2_string_interop() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!("ros2-string-interop fixture not built: {e}"),
    };
    let locator = zenohd_unique.locator();

    // nano-ros subscriber first, so its /chatter subscription is declared
    // before the stock talker starts publishing.
    let mut sub_cmd = std::process::Command::new(&sub_bin);
    sub_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut sub =
        ManagedProcess::spawn_command(sub_cmd, "nros-string-sub").expect("spawn nano-ros sub");
    sub.wait_for_output_pattern("Waiting for", Duration::from_secs(8))
        .expect("nano-ros subscriber did not become ready");

    // Stock, unmodified upstream rclcpp talker over rmw_zenoh_cpp.
    let mut _talker = Ros2Process::demo_nodes_cpp_talker(&locator, DEFAULT_ROS_DISTRO)
        .expect("spawn demo_nodes_cpp talker");

    let out = sub
        .wait_for_output_count("Received:", 2, Duration::from_secs(20))
        .expect("nano-ros subscriber received nothing from the stock demo_nodes_cpp talker");

    sub.kill();
    _talker.kill();

    let received = nros_tests::count_pattern(&out, "Received:");
    assert!(
        received >= 2,
        "nano-ros must receive the stock demo_nodes_cpp talker's std_msgs/String \
         cross-vendor (Received = {received}):\n{out}"
    );
}
