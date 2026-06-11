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

use nros_tests::{
    count_pattern,
    fixtures::{
        DEFAULT_ROS_DISTRO, ManagedProcess, Ros2DdsProcess, XrceAgent, is_rmw_fastrtps_available,
        is_ros2_available, require_ros2_dds, require_xrce_agent, xrce_action_client_binary,
        xrce_action_server_binary, xrce_listener_binary, xrce_service_client_binary,
        xrce_service_server_binary, xrce_talker_binary,
    },
    unique_ros_domain_id,
};
use rstest::rstest;
use std::{path::PathBuf, time::Duration};

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

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_ros2_dds() {
        nros_tests::skip!("ROS 2 DDS not available");
    }

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // Start ROS 2 DDS listener (uses rmw_fastrtps_cpp, DDS multicast discovery)
    eprintln!("Starting ROS 2 DDS topic echo...");
    let mut ros2_listener = match Ros2DdsProcess::topic_echo_with_domain(
        "/chatter",
        "std_msgs/msg/Int32",
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            nros_tests::skip!(
                "ROS 2 DDS listener could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    // Wait for DDS discovery + XRCE Agent to propagate
    std::thread::sleep(Duration::from_secs(1));

    // Start XRCE talker
    eprintln!("Starting XRCE talker...");
    let mut talker_cmd = Command::new(&xrce_talker_binary);
    talker_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string());
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

    // Drop the agent before asserting so a failure doesn't leak the process.
    drop(agent);

    assert!(
        received_count > 0,
        "ROS 2 DDS listener received no messages from the XRCE talker — the \
         header-stripped XRCE topic payload must parse as DDS CDR (233.6). Output:\n{}",
        ros2_output
    );
    eprintln!("[PASS] XRCE → ROS 2 DDS pub/sub works");
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

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_ros2_dds() {
        nros_tests::skip!("ROS 2 DDS not available");
    }

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // Start XRCE listener (subscribe before publishing)
    eprintln!("Starting XRCE listener...");
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    listener_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_MSG_COUNT", "3")
        // "Received:" logs at INFO — surface them so the test can see receipt.
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-listener")
        .expect("Failed to start listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5));

    // Wait for DDS discovery + XRCE Agent to propagate subscription
    std::thread::sleep(Duration::from_secs(1));

    // Start ROS 2 DDS publisher
    eprintln!("Starting ROS 2 DDS topic pub...");
    let mut ros2_publisher = match Ros2DdsProcess::topic_pub_with_domain(
        "/chatter",
        "std_msgs/msg/Int32",
        "{data: 42}",
        2,
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            listener.kill();
            nros_tests::skip!(
                "ROS 2 DDS publisher could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
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

    drop(agent);

    assert!(
        received_count > 0,
        "XRCE listener received no messages from the ROS 2 DDS publisher — the \
         re-prepended CDR header on the inbound XRCE topic must parse (233.6). Output:\n{}",
        listener_output
    );
    eprintln!("[PASS] ROS 2 DDS → XRCE pub/sub works");
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

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_ros2_dds() {
        nros_tests::skip!("ROS 2 DDS not available");
    }

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // Start XRCE service server
    eprintln!("Starting XRCE service server...");
    let mut server_cmd = Command::new(&xrce_service_server_binary);
    server_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-service-server")
        .expect("Failed to start service server");

    // Wait for server to be ready
    let _ = server.wait_for_output_pattern("Service server ready", Duration::from_secs(5));

    // Wait for DDS discovery to propagate the service
    std::thread::sleep(Duration::from_secs(1));

    // Call service from ROS 2 DDS
    eprintln!("Calling service from ROS 2 DDS...");
    let mut ros2_client = match Ros2DdsProcess::service_call_with_domain(
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "{a: 5, b: 3}",
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::skip!(
                "ROS 2 DDS service call could not start (missing ROS 2 tooling?): {}",
                e
            );
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

    drop(agent);

    // Phase 233.6 — this direction (real ROS 2 service client ↔ nano-ros XRCE
    // service server) is the acceptance for the XRCE-DDS service CDR-header
    // interop fix. Hard-assert (unlike the other diagnostic interop tests): a
    // ROS 2 `rmw_fastrtps` client must get the correct `sum=8` reply. Before the
    // header strip/prepend in `service.c` the request deserialized misaligned
    // and the server never replied with a valid value.
    assert!(
        has_sum && has_correct_value,
        "ROS 2 service client did not get sum=8 from the nano-ros XRCE service \
         server — XRCE-DDS service interop regression (233.6). Output:\n{ros2_output}"
    );
    eprintln!("[PASS] XRCE service server ↔ ROS 2 DDS client: sum=8 verified");
}

// =============================================================================
// Phase 183.6 — XRCE ↔ ROS 2: action (both directions) + reverse-direction
// service. The existing tests cover pub/sub both ways + service
// (xrce-server / ros2-client). These add the missing cells. DDS interop is
// best-effort (naming/version drift), so — like the tests above — they log
// PASS/INFO and only hard-fail on a clear local error, never on a discovery
// miss. nano-XRCE nodes bridge to DDS via the XRCE Agent; ROS 2 uses
// rmw_fastrtps_cpp on the same ROS_DOMAIN_ID.
// =============================================================================

/// nano-XRCE action server ↔ ROS 2 (DDS) action client (`ros2 action send_goal`).
#[rstest]
fn test_xrce_action_ros2_client(xrce_action_server_binary: PathBuf) {
    use std::process::Command;
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    if !require_ros2_dds() {
        nros_tests::skip!("ROS 2 DDS not available");
    }
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    let mut server_cmd = Command::new(&xrce_action_server_binary);
    server_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-action-server")
        .expect("Failed to start xrce action server");
    let _ = server.wait_for_output_pattern("Action server ready", Duration::from_secs(8));
    std::thread::sleep(Duration::from_secs(1));

    let mut ros2_client = match Ros2DdsProcess::action_send_goal_with_domain(
        "/fibonacci",
        "example_interfaces/action/Fibonacci",
        "{order: 5}",
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::skip!(
                "ROS 2 DDS action client could not start (requires ros-humble-example-interfaces): {e}"
            );
        }
    };
    let ros2_output = ros2_client
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    server.kill();
    drop(agent);

    eprintln!("ROS 2 DDS action client output:\n{ros2_output}");
    // A real `ros2 action send_goal --feedback` against the nano-ros XRCE
    // action server accepts the goal, streams feedback, and delivers the final
    // result — exercising the send_goal/get_result services + feedback topic
    // with ROS-2-correct names/types, the fixed `uint8[16]` goal-id framing
    // (233.6), and the deferred get_result reply (Phase 237): rclcpp_action
    // sends get_result right after acceptance and the server holds the reply
    // until the goal terminates.
    let accepted = ros2_output.contains("Goal accepted");
    let got_feedback = ros2_output.contains("- 0");
    let got_result = ros2_output.contains("SUCCEEDED") || ros2_output.contains("Result:");
    assert!(
        accepted && got_feedback && got_result,
        "XRCE action server ↔ ROS 2 DDS client did not complete (233.6 / 237): \
         accepted={accepted} feedback={got_feedback} result={got_result}.\n{ros2_output}"
    );
    eprintln!("[PASS] XRCE action server ↔ ROS 2 DDS client: accept + feedback + result");
}

/// ROS 2 (DDS) action server ↔ nano-XRCE action client (reverse direction).
#[rstest]
fn test_ros2_action_xrce_client(xrce_action_client_binary: PathBuf) {
    use std::process::Command;
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    if !require_ros2_dds() {
        nros_tests::skip!("ROS 2 DDS not available");
    }
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    let mut ros2_server = match Ros2DdsProcess::action_server_fibonacci_with_domain(
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            nros_tests::skip!(
                "ROS 2 DDS fibonacci action server could not start (action_tutorials_py not installed?): {e}"
            );
        }
    };
    // The rclpy ActionServer needs a few seconds to import rclpy, create the
    // 5 action entities, and announce them over DDS so the agent's requesters
    // can match before the client's send_goal fires (the action client has no
    // wait_for/retry — see the example's warmup loop). 3s was too tight under
    // test load and intermittently missed the match; 6s lands reliably.
    std::thread::sleep(Duration::from_secs(6));

    let mut client_cmd = Command::new(&xrce_action_client_binary);
    client_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30")
        // Action result / status logs are INFO-level — surface them.
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-action-client")
        .expect("Failed to start xrce action client");
    // The example logs "Final sequence" once it has streamed the full
    // Fibonacci feedback; "Action client finished" is its terminal line.
    let client_output = client
        .wait_for_output_pattern("Action client finished", Duration::from_secs(20))
        .unwrap_or_default();
    client.kill();
    ros2_server.kill();
    drop(agent);

    eprintln!("XRCE action client output:\n{client_output}");
    // Goal acceptance proves the send_goal request crossed XRCE→agent→DDS and
    // the reply came back (goal_id framing + per-channel service types). At
    // least one non-empty feedback proves the feedback topic + UUID framing
    // round-trips from a real `rcl_action` server.
    let accepted = client_output.contains("Goal accepted");
    let got_feedback = client_output.contains("Feedback #1: [0, 1");
    assert!(
        accepted && got_feedback,
        "ROS 2 DDS action server ↔ XRCE action client did not complete (233.6): \
         accepted={accepted} got_feedback={got_feedback}.\n{client_output}"
    );
    eprintln!("[PASS] ROS 2 DDS action server ↔ XRCE action client: goal accepted + feedback");
}

/// ROS 2 (DDS) service server ↔ nano-XRCE service client (reverse direction).
#[rstest]
fn test_ros2_service_xrce_client(xrce_service_client_binary: PathBuf) {
    use std::process::Command;
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    if !require_ros2_dds() {
        nros_tests::skip!("ROS 2 DDS not available");
    }
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    let mut ros2_server = match Ros2DdsProcess::add_two_ints_server_with_domain(
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            nros_tests::skip!(
                "ROS 2 DDS add_two_ints server could not start (requires ros-humble-example-interfaces): {e}"
            );
        }
    };
    let _ = ros2_server.wait_for_output(Duration::from_secs(5)); // let it reach "Service server ready"
    std::thread::sleep(Duration::from_secs(1));

    let mut client_cmd = Command::new(&xrce_service_client_binary);
    client_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_REQUEST_COUNT", "3")
        .env("XRCE_TIMEOUT", "30")
        // The client logs "Response: ..." at INFO — surface it so the assert
        // below can see the reply (without this the test can't tell success
        // from failure).
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-service-client")
        .expect("Failed to start xrce service client");
    let client_output = client
        .wait_for_output_pattern("Response", Duration::from_secs(20))
        .unwrap_or_default();
    client.kill();
    ros2_server.kill();
    drop(agent);

    eprintln!("XRCE service client output:\n{client_output}");
    // Phase 233.6 wave 2 — reverse direction (nano-ros XRCE service client →
    // real ROS 2 service server). Hard assert: the client must get a reply.
    assert!(
        client_output.contains("Response") || client_output.contains("sum"),
        "nano-ros XRCE service client got no reply from the ROS 2 service server \
         — XRCE-DDS reverse service interop regression (233.6). Output:\n{client_output}"
    );
    eprintln!("[PASS] ROS 2 DDS service server ↔ XRCE service client: reply received");
}
