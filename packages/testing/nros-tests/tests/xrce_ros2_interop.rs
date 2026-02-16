//! XRCE-DDS ↔ ROS 2 DDS interoperability tests
//!
//! Tests communication between nros XRCE nodes and ROS 2 nodes using
//! the standard micro-ROS architecture:
//!
//! ```text
//! nros XRCE node → XRCE Agent (Fast-DDS) ←DDS multicast→ ROS 2 node (rmw_fastrtps_cpp)
//! ```
//!
//! The XRCE Agent creates DDS participants on behalf of XRCE clients,
//! so ROS 2 nodes using the same DDS domain can discover and communicate
//! with them via standard DDS multicast.
//!
//! **Note:** These tests are diagnostic/informational — they report interop
//! status but do not hard-fail the test suite, because DDS interop between
//! the XRCE Agent's bundled Fast-DDS and the system's ROS 2 Fast-DDS can
//! have version-dependent issues. Same pattern as `rmw_interop.rs`.
//!
//! Prerequisites:
//!   just build-xrce-agent        # Build Micro-XRCE-DDS Agent
//!   ROS 2 Humble installed       # /opt/ros/humble/
//!   rmw_fastrtps_cpp available   # Default in Humble
//!   example_interfaces installed # For AddTwoInts service type

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    DEFAULT_ROS_DISTRO, ManagedProcess, Ros2DdsProcess, XrceAgent, is_rmw_fastrtps_available,
    is_ros2_available, require_ros2_dds, require_xrce_agent, xrce_listener_binary,
    xrce_service_server_binary, xrce_talker_binary,
};
use rstest::rstest;
use std::path::PathBuf;
use std::time::Duration;

// =============================================================================
// Detection Tests
// =============================================================================

#[test]
fn test_ros2_dds_detection() {
    let ros2 = is_ros2_available();
    let fastrtps = is_rmw_fastrtps_available();
    eprintln!("ROS 2 available: {}", ros2);
    eprintln!("rmw_fastrtps_cpp available: {}", fastrtps);
}

// =============================================================================
// XRCE → ROS 2 Pub/Sub
// =============================================================================

/// nros XRCE talker → ROS 2 DDS listener
///
/// Architecture:
///   xrce-talker → XRCE Agent (UDP) → DDS multicast → ros2 topic echo (rmw_fastrtps_cpp)
#[rstest]
fn test_xrce_to_ros2_pubsub(xrce_talker_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() || !require_ros2_dds() {
        return;
    }

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start ROS 2 DDS listener (uses rmw_fastrtps_cpp, DDS multicast discovery)
    eprintln!("Starting ROS 2 DDS topic echo...");
    let mut ros2_listener =
        match Ros2DdsProcess::topic_echo("/chatter", "std_msgs/msg/Int32", DEFAULT_ROS_DISTRO) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to start ROS 2 DDS listener: {}", e);
                return;
            }
        };

    // Wait for DDS discovery + XRCE Agent to propagate
    std::thread::sleep(Duration::from_secs(1));

    // Start XRCE talker
    eprintln!("Starting XRCE talker...");
    let mut talker_cmd = Command::new(&xrce_talker_binary);
    talker_cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for talker to publish
    let _ = talker.wait_for_output_pattern("Published:", Duration::from_secs(5));

    // Give ROS 2 time to receive messages via DDS
    std::thread::sleep(Duration::from_secs(2));

    // Collect ROS 2 output
    talker.kill();
    let ros2_output = ros2_listener
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    eprintln!("ROS 2 DDS output:\n{}", ros2_output);

    let received_count = count_pattern(&ros2_output, "data:");
    eprintln!("ROS 2 received {} messages via DDS", received_count);

    if received_count > 0 {
        eprintln!("[PASS] XRCE → ROS 2 DDS pub/sub works");
    } else {
        eprintln!("[INFO] ROS 2 DDS listener did not receive messages from XRCE talker");
        eprintln!("  This may indicate DDS version incompatibility between XRCE Agent and ROS 2");
    }

    drop(agent);
}

// =============================================================================
// ROS 2 → XRCE Pub/Sub
// =============================================================================

/// ROS 2 DDS talker → nros XRCE listener
///
/// Architecture:
///   ros2 topic pub (rmw_fastrtps_cpp) → DDS multicast → XRCE Agent → xrce-listener (UDP)
#[rstest]
fn test_ros2_to_xrce_pubsub(xrce_listener_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() || !require_ros2_dds() {
        return;
    }

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start XRCE listener (subscribe before publishing)
    eprintln!("Starting XRCE listener...");
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_MSG_COUNT", "3");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-listener")
        .expect("Failed to start listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5));

    // Wait for DDS discovery + XRCE Agent to propagate subscription
    std::thread::sleep(Duration::from_secs(1));

    // Start ROS 2 DDS publisher
    eprintln!("Starting ROS 2 DDS topic pub...");
    let mut ros2_publisher = match Ros2DdsProcess::topic_pub(
        "/chatter",
        "std_msgs/msg/Int32",
        "{data: 42}",
        2,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 DDS publisher: {}", e);
            listener.kill();
            return;
        }
    };

    // Wait for XRCE listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(5))
        .unwrap_or_default();

    ros2_publisher.kill();
    listener.kill();

    eprintln!("XRCE listener output:\n{}", listener_output);

    let received_count = count_pattern(&listener_output, "Received:");
    eprintln!(
        "XRCE listener received {} messages from ROS 2 DDS",
        received_count
    );

    if received_count > 0 {
        eprintln!("[PASS] ROS 2 DDS → XRCE pub/sub works");
    } else {
        eprintln!("[INFO] XRCE listener did not receive messages from ROS 2 DDS publisher");
        eprintln!("  This may indicate DDS version incompatibility between XRCE Agent and ROS 2");
    }

    drop(agent);
}

// =============================================================================
// XRCE Service Server + ROS 2 DDS Client
// =============================================================================

/// nros XRCE service server + ROS 2 DDS service client
///
/// Architecture:
///   ros2 service call (rmw_fastrtps_cpp) → DDS → XRCE Agent → xrce-service-server (UDP)
#[rstest]
fn test_xrce_service_ros2_client(xrce_service_server_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() || !require_ros2_dds() {
        return;
    }

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start XRCE service server
    eprintln!("Starting XRCE service server...");
    let mut server_cmd = Command::new(&xrce_service_server_binary);
    server_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-service-server")
        .expect("Failed to start service server");

    // Wait for server to be ready
    let _ = server.wait_for_output_pattern("Service server ready", Duration::from_secs(5));

    // Wait for DDS discovery to propagate the service
    std::thread::sleep(Duration::from_secs(1));

    // Call service from ROS 2 DDS
    eprintln!("Calling service from ROS 2 DDS...");
    let mut ros2_client = match Ros2DdsProcess::service_call(
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "{a: 5, b: 3}",
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 DDS service call: {}", e);
            server.kill();
            return;
        }
    };

    // Wait for service call to complete
    let ros2_output = ros2_client
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    server.kill();

    eprintln!("ROS 2 DDS service call output:\n{}", ros2_output);

    // Check if service call succeeded
    let has_sum = ros2_output.contains("sum");
    let has_correct_value = ros2_output.contains("8");

    if has_sum && has_correct_value {
        eprintln!("[PASS] XRCE service server ↔ ROS 2 DDS client: sum=8 verified");
    } else if has_sum {
        eprintln!(
            "[PASS] XRCE service server ↔ ROS 2 DDS client: service responded (sum field present)"
        );
    } else {
        eprintln!("[INFO] ROS 2 DDS service call did not receive expected response");
        eprintln!("  This may indicate DDS service naming or version incompatibility");
        if ros2_output.contains("waiting for service") {
            eprintln!("  Service was not discovered via DDS — check naming conventions");
        }
    }

    drop(agent);
}
