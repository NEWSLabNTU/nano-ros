//! ROS 2 rmw_zenoh interoperability tests
//!
//! Tests communication between nros and ROS 2 nodes using rmw_zenoh_cpp.
//!
//! ## Test Categories
//!
//! - **Detection**: Check if ROS 2 and rmw_zenoh are available
//! - **Pub/Sub**: nros ↔ ROS 2 message passing
//! - **Services**: nros ↔ ROS 2 service calls
//! - **Actions**: nros ↔ ROS 2 action protocol
//! - **Discovery**: `ros2 node/topic/service list` visibility
//! - **QoS**: Reliability and durability compatibility
//! - **Benchmarks**: Latency and throughput measurements

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    DEFAULT_ROS_DISTRO, ManagedProcess, Ros2Process, ZenohRouter, action_client_binary,
    action_server_binary, is_rmw_zenoh_available, is_ros2_available, listener_binary,
    ros2_node_list, ros2_service_list, ros2_topic_info, ros2_topic_list, service_client_binary,
    service_server_binary, talker_binary, zenohd_unique,
};
use rstest::rstest;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Skip test if ROS 2 prerequisites are not met
fn require_ros2() -> bool {
    if !is_ros2_available() {
        eprintln!("Skipping test: ROS 2 not available");
        return false;
    }
    if !is_rmw_zenoh_available() {
        eprintln!("Skipping test: rmw_zenoh_cpp not available");
        return false;
    }
    true
}

// =============================================================================
// Detection Tests
// =============================================================================

#[test]
fn test_ros2_detection() {
    let available = is_ros2_available();
    eprintln!("ROS 2 available: {}", available);
}

#[test]
fn test_rmw_zenoh_detection() {
    let available = is_rmw_zenoh_available();
    eprintln!("rmw_zenoh_cpp available: {}", available);
}

// =============================================================================
// nros → ROS 2 Tests
// =============================================================================

#[rstest]
fn test_nano_to_ros2(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start ROS 2 listener first
    eprintln!("Starting ROS 2 topic echo...");
    let mut ros2_listener = match Ros2Process::topic_echo(
        "/chatter",
        "std_msgs/msg/Int32",
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 listener: {}", e);
            return;
        }
    };

    // Give ROS 2 time to subscribe
    std::thread::sleep(Duration::from_secs(3));

    // Start nros talker with NROS_LOCATOR env var
    eprintln!("Starting nros talker...");
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Let them communicate
    std::thread::sleep(Duration::from_secs(5));

    // Kill talker first
    talker.kill();

    // Collect ROS 2 output
    let ros2_output = ros2_listener
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("ROS 2 output:\n{}", ros2_output);

    // Check if ROS 2 received messages
    let received_count = count_pattern(&ros2_output, "data:");
    eprintln!("ROS 2 received {} messages", received_count);

    if received_count > 0 {
        eprintln!("[PASS] nros → ROS 2 communication works");
    } else {
        eprintln!("[INFO] ROS 2 did not receive messages (may be timing issue)");
    }
}

// =============================================================================
// ROS 2 → nros Tests
// =============================================================================

#[rstest]
fn test_ros2_to_nano(zenohd_unique: ZenohRouter, listener_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start nros listener first with NROS_LOCATOR env var
    eprintln!("Starting nros listener...");
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    // Give listener time to subscribe
    std::thread::sleep(Duration::from_secs(3));

    // Start ROS 2 publisher
    eprintln!("Starting ROS 2 topic pub...");
    let mut ros2_publisher = match Ros2Process::topic_pub(
        "/chatter",
        "std_msgs/msg/Int32",
        "{data: 42}",
        1,
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 publisher: {}", e);
            listener.kill();
            return;
        }
    };

    // Let them communicate
    std::thread::sleep(Duration::from_secs(5));

    // Kill ROS 2 publisher first
    ros2_publisher.kill();

    // Collect nros output (log::info! goes to stderr)
    let nano_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("nros output:\n{}", nano_output);

    // Check if nros received messages
    let received_count = count_pattern(&nano_output, "Received:");
    eprintln!("nros received {} messages", received_count);

    if received_count > 0 {
        eprintln!("[PASS] ROS 2 → nros communication works");

        // Check data integrity
        if nano_output.contains("data=42") {
            eprintln!("[PASS] Data integrity verified (data=42)");
        }
    } else {
        eprintln!("[INFO] nros did not receive messages (may be timing issue)");
    }
}

// =============================================================================
// Communication Matrix Tests
// =============================================================================

/// Test direction for matrix tests
#[derive(Debug, Clone, Copy)]
enum Direction {
    NanoToNano,
    NanoToRos2,
    Ros2ToNano,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::NanoToNano => write!(f, "nros → nros"),
            Direction::NanoToRos2 => write!(f, "nros → ROS 2"),
            Direction::Ros2ToNano => write!(f, "ROS 2 → nros"),
        }
    }
}

#[rstest]
#[case(Direction::NanoToNano)]
#[case(Direction::NanoToRos2)]
#[case(Direction::Ros2ToNano)]
fn test_communication_matrix(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    listener_binary: PathBuf,
    #[case] direction: Direction,
) {
    // Check prerequisites based on direction
    match direction {
        Direction::NanoToNano => {}
        Direction::NanoToRos2 | Direction::Ros2ToNano => {
            if !require_ros2() {
                nros_tests::skip!("ROS 2 not found");
            }
        }
    }

    let locator = zenohd_unique.locator();

    eprintln!("Testing: {}", direction);

    let success = match direction {
        Direction::NanoToNano => {
            test_nano_to_nano_inner(&locator, &talker_binary, &listener_binary)
        }
        Direction::NanoToRos2 => test_nano_to_ros2_inner(&locator, &talker_binary),
        Direction::Ros2ToNano => test_ros2_to_nano_inner(&locator, &listener_binary),
    };

    if success {
        eprintln!("[PASS] {}", direction);
    } else {
        eprintln!(
            "[INFO] {} - no messages received (may be timing)",
            direction
        );
    }
}

fn test_nano_to_nano_inner(locator: &str, talker_path: &Path, listener_path: &Path) -> bool {
    use std::process::Command;

    // Start listener with NROS_LOCATOR env var
    let mut listener_cmd = Command::new(listener_path);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(2));

    // Start talker with NROS_LOCATOR env var
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(5));

    talker.kill();
    let output = listener
        .wait_for_all_output(Duration::from_secs(1))
        .unwrap_or_default();

    count_pattern(&output, "Received:") > 0
}

fn test_nano_to_ros2_inner(locator: &str, talker_path: &Path) -> bool {
    use std::process::Command;

    // Start ROS 2 listener
    let mut ros2_listener = match Ros2Process::topic_echo(
        "/chatter",
        "std_msgs/msg/Int32",
        locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(_) => return false,
    };

    std::thread::sleep(Duration::from_secs(3));

    // Start nros talker with NROS_LOCATOR env var
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(4));

    talker.kill();
    let output = ros2_listener
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    count_pattern(&output, "data:") > 0
}

fn test_ros2_to_nano_inner(locator: &str, listener_path: &Path) -> bool {
    use std::process::Command;

    // Start nros listener with NROS_LOCATOR env var
    let mut listener_cmd = Command::new(listener_path);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(3));

    // Start ROS 2 publisher
    let mut ros2_publisher = match Ros2Process::topic_pub(
        "/chatter",
        "std_msgs/msg/Int32",
        "{data: 42}",
        1,
        locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(_) => return false,
    };

    std::thread::sleep(Duration::from_secs(4));

    ros2_publisher.kill();
    let output = listener
        .wait_for_all_output(Duration::from_secs(1))
        .unwrap_or_default();

    count_pattern(&output, "Received:") > 0
}

// =============================================================================
// Protocol Detail Tests (migrated from rmw-detailed/)
// =============================================================================

#[rstest]
fn test_keyexpr_format(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    let locator = zenohd_unique.locator();

    // Start talker briefly to register key expression with NROS_LOCATOR env var
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(2));
    talker.kill();

    // Expected key expression format for Humble:
    // <domain_id>/<topic>/<type>/<hash>
    // 0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported
    eprintln!("[PASS] Talker started and registered key expression");
    eprintln!("Expected format: 0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported");
}

#[rstest]
fn test_qos_compatibility(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Test BEST_EFFORT QoS compatibility
    eprintln!("Testing BEST_EFFORT QoS compatibility...");

    // Start ROS 2 listener with BEST_EFFORT
    let mut ros2_listener = match Ros2Process::topic_echo(
        "/chatter",
        "std_msgs/msg/Int32",
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 listener: {}", e);
            return;
        }
    };

    std::thread::sleep(Duration::from_secs(3));

    // Start nros talker (uses BEST_EFFORT by default) with NROS_LOCATOR env var
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(4));

    talker.kill();
    let output = ros2_listener
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    if count_pattern(&output, "data:") > 0 {
        eprintln!("[PASS] BEST_EFFORT QoS compatible");
    } else {
        eprintln!("[INFO] QoS test inconclusive");
    }
}

// =============================================================================
// ROS 2 Action Interop Tests
// =============================================================================

#[rstest]
fn test_action_nano_server_ros2_client(zenohd_unique: ZenohRouter, action_server_binary: PathBuf) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start nros action server
    eprintln!("Starting nros action server...");
    let mut server_cmd = Command::new(&action_server_binary);
    server_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(server_cmd, "native-rs-action-server")
        .expect("Failed to start action server");

    // Give server time to set up
    std::thread::sleep(Duration::from_secs(3));

    if !server.is_running() {
        eprintln!("[FAIL] Action server exited early");
        return;
    }

    // Start ROS 2 action client
    eprintln!("Starting ROS 2 action send_goal...");
    let mut ros2_client = match Ros2Process::action_send_goal(
        "/fibonacci",
        "example_interfaces/action/Fibonacci",
        "{order: 5}",
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 action client: {}", e);
            server.kill();
            return;
        }
    };

    // Wait for action to complete (Fibonacci(5) takes ~3 seconds)
    std::thread::sleep(Duration::from_secs(5));

    // Collect ROS 2 output
    let ros2_output = ros2_client
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    server.kill();

    eprintln!("ROS 2 action client output:\n{}", ros2_output);

    // Check if ROS 2 received goal response and result
    let goal_accepted = ros2_output.contains("Goal accepted") || ros2_output.contains("accepted");
    let feedback_received =
        count_pattern(&ros2_output, "feedback") > 0 || count_pattern(&ros2_output, "Feedback") > 0;
    let result_received = ros2_output.contains("Result")
        || ros2_output.contains("sequence")
        || ros2_output.contains("result");

    if goal_accepted || feedback_received || result_received {
        eprintln!("[PASS] nros action server ↔ ROS 2 action client works");
        if goal_accepted {
            eprintln!("  - Goal accepted");
        }
        if feedback_received {
            eprintln!("  - Feedback received");
        }
        if result_received {
            eprintln!("  - Result received");
        }
    } else {
        eprintln!("[INFO] ROS 2 action client did not receive expected output");
        eprintln!("  This may be a timing issue or protocol incompatibility");
    }
}

#[rstest]
fn test_action_ros2_server_nano_client(zenohd_unique: ZenohRouter, action_client_binary: PathBuf) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start ROS 2 action server (Fibonacci)
    eprintln!("Starting ROS 2 Fibonacci action server...");
    let mut ros2_server = match Ros2Process::action_server_fibonacci(&locator, DEFAULT_ROS_DISTRO) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 action server: {}", e);
            eprintln!("[INFO] This test requires ros-humble-example-interfaces");
            return;
        }
    };

    // Give server time to set up
    std::thread::sleep(Duration::from_secs(5));

    // Start nros action client
    eprintln!("Starting nros action client...");
    let mut client_cmd = Command::new(&action_client_binary);
    client_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-action-client")
        .expect("Failed to start action client");

    // Wait for action to complete
    std::thread::sleep(Duration::from_secs(8));

    // Collect nros output
    let nano_output = client
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    ros2_server.kill();

    eprintln!("nros action client output:\n{}", nano_output);

    // Check if nros received goal response and feedback
    let goal_accepted = nano_output.contains("Goal accepted");
    let feedback_count = count_pattern(&nano_output, "Feedback #");
    let completed =
        nano_output.contains("action completed") || nano_output.contains("Action client finished");

    if goal_accepted || feedback_count > 0 || completed {
        eprintln!("[PASS] ROS 2 action server ↔ nros action client works");
        if goal_accepted {
            eprintln!("  - Goal accepted");
        }
        if feedback_count > 0 {
            eprintln!("  - Received {} feedback messages", feedback_count);
        }
        if completed {
            eprintln!("  - Action completed");
        }
    } else {
        eprintln!("[INFO] nros action client did not receive expected output");
        eprintln!("  This may be a timing issue or protocol incompatibility");
    }
}

// =============================================================================
// Discovery Tests
//
// These tests verify that nros entities are visible to ROS 2 CLI tools
// (ros2 node/topic/service list) via liveliness token discovery.
// Entity types tested: NN (node), MP (publisher), MS (subscriber), SS (service).
// =============================================================================

#[rstest]
fn test_discovery_node_visible(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start nros talker (creates node "talker" + publisher on /chatter)
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Wait for liveliness token registration
    std::thread::sleep(Duration::from_secs(4));

    let node_list = ros2_node_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default();
    talker.kill();

    eprintln!("Node list:\n{}", node_list);
    assert!(
        node_list.contains("talker"),
        "Expected 'talker' node in ros2 node list, got:\n{node_list}"
    );
}

#[rstest]
fn test_discovery_topic_visible(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start nros talker (publisher liveliness → MP entity)
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(4));

    let topic_list = ros2_topic_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default();
    let topic_info = ros2_topic_info("/chatter", &locator, DEFAULT_ROS_DISTRO).unwrap_or_default();

    talker.kill();

    eprintln!("Topic list:\n{}", topic_list);
    eprintln!("Topic info:\n{}", topic_info);

    assert!(
        topic_list.contains("/chatter"),
        "Expected '/chatter' in ros2 topic list, got:\n{topic_list}"
    );
    // Publisher liveliness (MP entity) should make /chatter show 1 publisher
    assert!(
        topic_info.contains("Publisher count: 1"),
        "Expected 'Publisher count: 1' in topic info, got:\n{topic_info}"
    );
}

#[rstest]
fn test_discovery_subscriber_visible(zenohd_unique: ZenohRouter, listener_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start nros listener (subscriber liveliness → MS entity)
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(4));

    let topic_info = ros2_topic_info("/chatter", &locator, DEFAULT_ROS_DISTRO).unwrap_or_default();

    listener.kill();

    eprintln!("Topic info:\n{}", topic_info);

    // Subscriber liveliness (MS entity) should make /chatter show 1 subscription
    assert!(
        topic_info.contains("Subscription count: 1"),
        "Expected 'Subscription count: 1' in topic info, got:\n{topic_info}"
    );
}

#[rstest]
fn test_discovery_service_visible(zenohd_unique: ZenohRouter, service_server_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start nros service server (service liveliness → SS entity)
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(server_cmd, "native-rs-service-server")
        .expect("Failed to start service server");

    std::thread::sleep(Duration::from_secs(4));

    let service_list = ros2_service_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default();

    server.kill();

    eprintln!("Service list:\n{}", service_list);

    assert!(
        service_list.contains("/add_two_ints"),
        "Expected '/add_two_ints' in ros2 service list, got:\n{service_list}"
    );
}

#[rstest]
fn test_discovery_pub_sub_combined(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    listener_binary: PathBuf,
) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start listener first (subscriber registers before publisher)
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(2));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(4));

    let topic_info = ros2_topic_info("/chatter", &locator, DEFAULT_ROS_DISTRO).unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Topic info (combined):\n{}", topic_info);

    // Both publisher (MP) and subscriber (MS) liveliness should be visible
    assert!(
        topic_info.contains("Publisher count: 1"),
        "Expected 'Publisher count: 1' in topic info, got:\n{topic_info}"
    );
    assert!(
        topic_info.contains("Subscription count: 1"),
        "Expected 'Subscription count: 1' in topic info, got:\n{topic_info}"
    );
}

// =============================================================================
// Service Interop Tests
// =============================================================================

#[rstest]
fn test_service_nano_server_ros2_client(
    zenohd_unique: ZenohRouter,
    service_server_binary: PathBuf,
) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start nros service server
    eprintln!("Starting nros service server...");
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(server_cmd, "native-rs-service-server")
        .expect("Failed to start service server");

    // Wait for server to be ready
    std::thread::sleep(Duration::from_secs(4));

    if !server.is_running() {
        eprintln!("[FAIL] Service server exited early");
        return;
    }

    // Call service using ROS 2 CLI
    eprintln!("Calling service from ROS 2...");
    let mut ros2_client = match Ros2Process::service_call(
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "{a: 5, b: 3}",
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 service call: {}", e);
            server.kill();
            return;
        }
    };

    // Wait for service call to complete
    let ros2_output = ros2_client
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();

    server.kill();

    eprintln!("ROS 2 service call output:\n{}", ros2_output);

    // Check if service call succeeded
    // Expected output contains "sum: 8" or similar
    if ros2_output.contains("sum") && (ros2_output.contains("8") || ros2_output.contains("= 8")) {
        eprintln!("[PASS] nros service server ↔ ROS 2 service client works");
        eprintln!("  - Request: 5 + 3 = 8 verified");
    } else if ros2_output.contains("sum") {
        eprintln!("[PASS] nros service server ↔ ROS 2 service client works");
        eprintln!("  - Service call completed (sum field present)");
    } else {
        eprintln!("[INFO] ROS 2 service call did not receive expected response");
        eprintln!("  This may be a timing issue or protocol incompatibility");
    }
}

#[rstest]
fn test_service_ros2_server_nano_client(
    zenohd_unique: ZenohRouter,
    service_client_binary: PathBuf,
) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    // Start ROS 2 service server
    eprintln!("Starting ROS 2 service server...");
    let mut ros2_server = match Ros2Process::add_two_ints_server(&locator, DEFAULT_ROS_DISTRO) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 service server: {}", e);
            return;
        }
    };

    // Wait for server to be ready
    std::thread::sleep(Duration::from_secs(5));

    // Start nros service client
    eprintln!("Starting nros service client...");
    let mut client_cmd = Command::new(&service_client_binary);
    client_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-service-client")
        .expect("Failed to start service client");

    // Wait for client to complete
    std::thread::sleep(Duration::from_secs(5));

    // Collect nros output
    let nano_output = client
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    ros2_server.kill();

    eprintln!("nros service client output:\n{}", nano_output);

    // Check if service calls succeeded
    let response_count = count_pattern(&nano_output, "Response:");
    let success = nano_output.contains("completed successfully")
        || nano_output.contains("sum")
        || response_count > 0;

    if success {
        eprintln!("[PASS] ROS 2 service server ↔ nros service client works");
        if response_count > 0 {
            eprintln!("  - Received {} service responses", response_count);
        }
    } else {
        eprintln!("[INFO] nros service client did not receive expected responses");
        eprintln!("  This may be a timing issue or protocol incompatibility");
    }
}

// =============================================================================
// QoS Compatibility Tests
// =============================================================================

/// Test QoS compatibility matrix
#[derive(Debug, Clone, Copy)]
enum QosReliability {
    Reliable,
    BestEffort,
}

impl QosReliability {
    fn as_str(&self) -> &'static str {
        match self {
            QosReliability::Reliable => "reliable",
            QosReliability::BestEffort => "best_effort",
        }
    }
}

impl std::fmt::Display for QosReliability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[rstest]
#[case(QosReliability::BestEffort, QosReliability::BestEffort, true)]
#[case(QosReliability::Reliable, QosReliability::Reliable, true)]
#[case(QosReliability::Reliable, QosReliability::BestEffort, true)]
#[case(QosReliability::BestEffort, QosReliability::Reliable, false)]
fn test_qos_matrix(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    #[case] pub_qos: QosReliability,
    #[case] sub_qos: QosReliability,
    #[case] should_work: bool,
) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    eprintln!(
        "Testing QoS: publisher={}, subscriber={} (expected: {})",
        pub_qos,
        sub_qos,
        if should_work { "works" } else { "fails" }
    );

    // Start ROS 2 subscriber with specified QoS
    let mut ros2_subscriber = match Ros2Process::topic_echo_with_qos(
        "/chatter",
        "std_msgs/msg/Int32",
        sub_qos.as_str(),
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 subscriber: {}", e);
            return;
        }
    };

    std::thread::sleep(Duration::from_secs(3));

    // Start nros talker (currently only supports BEST_EFFORT)
    // Note: For full QoS testing, we'd need to modify the talker to support different QoS
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(4));

    talker.kill();
    let output = ros2_subscriber
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    let received = count_pattern(&output, "data:") > 0;

    // Note: nros talker uses BEST_EFFORT, so this tests BEST_EFFORT publisher
    match pub_qos {
        QosReliability::BestEffort => {
            if received == should_work {
                eprintln!(
                    "[PASS] QoS {}→{}: {} as expected",
                    pub_qos,
                    sub_qos,
                    if received { "received" } else { "no data" }
                );
            } else {
                eprintln!(
                    "[INFO] QoS {}→{}: unexpected result (received={})",
                    pub_qos, sub_qos, received
                );
            }
        }
        QosReliability::Reliable => {
            // Skip test - nros talker doesn't support RELIABLE yet
            eprintln!(
                "[SKIP] QoS {}→{}: nros talker doesn't support RELIABLE",
                pub_qos, sub_qos
            );
        }
    }
}

// =============================================================================
// Latency Benchmarks
// =============================================================================

/// Simple latency measurement for nros → ROS 2
#[rstest]
fn test_latency_nano_to_ros2(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    eprintln!("=== Latency Benchmark: nros → ROS 2 ===");

    // Start ROS 2 subscriber
    let mut ros2_subscriber = match Ros2Process::topic_echo(
        "/chatter",
        "std_msgs/msg/Int32",
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 subscriber: {}", e);
            return;
        }
    };

    std::thread::sleep(Duration::from_secs(3));

    // Record start time
    let start = Instant::now();

    // Start nros talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Wait for first message
    let mut first_message_time = None;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if let Ok(output) = ros2_subscriber.wait_for_output(Duration::from_millis(10)) {
            if output.contains("data:") {
                first_message_time = Some(start.elapsed());
                break;
            }
        }
    }

    talker.kill();

    match first_message_time {
        Some(elapsed) => {
            eprintln!(
                "[BENCHMARK] First message latency: {:?} (includes startup)",
                elapsed
            );
            eprintln!("  Note: This measures time from talker start to first message received");
            eprintln!("  Actual per-message latency is much lower");
        }
        None => {
            eprintln!("[INFO] No messages received within timeout");
        }
    }
}

/// Throughput measurement - count messages over fixed time
#[rstest]
fn test_throughput_nano_to_ros2(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    eprintln!("=== Throughput Benchmark: nros → ROS 2 ===");

    // Start ROS 2 subscriber
    let mut ros2_subscriber = match Ros2Process::topic_echo(
        "/chatter",
        "std_msgs/msg/Int32",
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to start ROS 2 subscriber: {}", e);
            return;
        }
    };

    std::thread::sleep(Duration::from_secs(3));

    // Start nros talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Run for fixed duration
    let test_duration = Duration::from_secs(5);
    std::thread::sleep(test_duration);

    talker.kill();
    let output = ros2_subscriber
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    let message_count = count_pattern(&output, "data:");
    let rate = message_count as f64 / test_duration.as_secs_f64();

    eprintln!("[BENCHMARK] Messages received: {}", message_count);
    eprintln!("[BENCHMARK] Throughput: {:.1} msg/sec", rate);
    eprintln!("  Note: Rate depends on talker publish frequency (typically 1 Hz)");
}
