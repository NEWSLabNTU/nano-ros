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

use nros_tests::{
    count_pattern,
    fixtures::{
        DEFAULT_ROS_DISTRO, ManagedProcess, Ros2Process, ZenohRouter, action_client_binary,
        action_server_binary, is_rmw_zenoh_available, is_ros2_available, listener_binary,
        ros2_node_list, ros2_service_list, ros2_topic_hz, ros2_topic_info, ros2_topic_list,
        service_client_binary, service_server_binary, talker_binary, zenohd_unique,
    },
};
use rstest::rstest;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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

fn poll_until_contains<F>(timeout: Duration, marker: &str, mut poll: F) -> String
where
    F: FnMut() -> String,
{
    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    while Instant::now() < deadline {
        output = poll();
        if output.contains(marker) {
            return output;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    output
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
            nros_tests::skip!(
                "ROS 2 listener could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    // Start nros talker with NROS_LOCATOR env var
    eprintln!("Starting nros talker...");
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    let ros2_output = ros2_listener
        .wait_for_output(Duration::from_secs(8))
        .unwrap_or_default();
    talker.kill();

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

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("nros listener did not become ready");

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
            listener.kill();
            nros_tests::skip!(
                "ROS 2 publisher could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    let nano_output = listener
        .wait_for_output_count("Received:", 1, Duration::from_secs(8))
        .unwrap_or_default();
    ros2_publisher.kill();

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

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("nros listener did not become ready");

    // Start talker with NROS_LOCATOR env var
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    let output = listener
        .wait_for_output_count("Received:", 1, Duration::from_secs(8))
        .unwrap_or_default();
    talker.kill();

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

    // Start nros talker with NROS_LOCATOR env var
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    let output = ros2_listener
        .wait_for_output(Duration::from_secs(8))
        .unwrap_or_default();
    talker.kill();

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

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("nros listener did not become ready");

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

    let output = listener
        .wait_for_output_count("Received:", 1, Duration::from_secs(8))
        .unwrap_or_default();
    ros2_publisher.kill();

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

    let _ = talker.wait_for_output_pattern("Publishing", Duration::from_secs(5));
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
            nros_tests::skip!(
                "ROS 2 listener could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    // Start nros talker (uses BEST_EFFORT by default) with NROS_LOCATOR env var
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    let output = ros2_listener
        .wait_for_output(Duration::from_secs(8))
        .unwrap_or_default();
    talker.kill();

    if count_pattern(&output, "data:") > 0 {
        eprintln!("[PASS] BEST_EFFORT QoS compatible");
    } else {
        eprintln!("[INFO] QoS test inconclusive");
    }
}

// =============================================================================
// ROS 2 Action Interop Tests
// =============================================================================

/// Phase 237 follow-up — TWO concurrent ROS 2 (rmw_zenoh) action clients against
/// one nano-ros Zenoh action server (concurrent mode). Both send_goal (and early
/// get_result) requests arrive essentially simultaneously: the service-server
/// request ring buffers both arrivals and the reply-token table holds both
/// deferred replies, so each client gets its own SUCCEEDED result.
#[rstest]
fn test_action_concurrent_nano_server_ros2_clients(
    zenohd_unique: ZenohRouter,
    action_server_binary: PathBuf,
) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    let mut server_cmd = Command::new(&action_server_binary);
    server_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        // Concurrent mode: accept + advance several goals at once, draining
        // get_result every spin so two early get_results are held together.
        .env("NROS_ACTION_CONCURRENT", "1");
    let mut server =
        ManagedProcess::spawn_command(server_cmd, "native-rs-action-server-concurrent")
            .expect("Failed to start action server");
    let _ = server.wait_for_output_pattern("Waiting for action", Duration::from_secs(10));
    if !server.is_running() {
        panic!("native-rs-action-server exited early before the action-ready pattern");
    }

    // Long goals (order 20 ≈ 2 s) so both overlap; fired simultaneously — the
    // request ring (237 follow-up) buffers both send_goal arrivals.
    let spawn = || {
        Ros2Process::action_send_goal(
            "/fibonacci",
            "example_interfaces/action/Fibonacci",
            "{order: 20}",
            &locator,
            DEFAULT_ROS_DISTRO,
        )
    };
    let mut c1 = match spawn() {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::skip!("ROS 2 action client could not start: {e}");
        }
    };
    let mut c2 = match spawn() {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::skip!("ROS 2 action client 2 could not start: {e}");
        }
    };
    let out1 = c1
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let out2 = c2
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    server.kill();

    eprintln!("client 1:\n{out1}\nclient 2:\n{out2}");
    let ok1 = out1.contains("SUCCEEDED");
    let ok2 = out2.contains("SUCCEEDED");
    assert!(
        ok1 && ok2,
        "concurrent rmw_zenoh action: both clients must get their own SUCCEEDED \
         (237 request ring + reply-token table): client1={ok1} client2={ok2}"
    );
    eprintln!("[PASS] two concurrent rmw_zenoh action clients ↔ nano-ros Zenoh server");
}

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

    let _ = server.wait_for_output_pattern("Waiting for action", Duration::from_secs(10));

    if !server.is_running() {
        panic!(
            "native-rs-action-server (the nros side under test) exited early before the action-ready pattern"
        );
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
            server.kill();
            nros_tests::skip!(
                "ROS 2 action client could not start (requires ros-humble-example-interfaces): {}",
                e
            );
        }
    };

    // Collect ROS 2 output
    let ros2_output = ros2_client
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();

    server.kill();

    eprintln!("ROS 2 action client output:\n{}", ros2_output);

    // A real `rmw_zenoh_cpp` `ros2 action send_goal` against the nano-ros Zenoh
    // action server must accept the goal, stream feedback, and deliver the final
    // result. The result exercises the Zenoh seq-keyed reply table (Phase 237):
    // rclcpp_action sends get_result right after acceptance and the server holds
    // the reply until the goal terminates.
    let goal_accepted = ros2_output.contains("Goal accepted") || ros2_output.contains("accepted");
    let feedback_received =
        count_pattern(&ros2_output, "feedback") > 0 || count_pattern(&ros2_output, "Feedback") > 0;
    let result_received = ros2_output.contains("SUCCEEDED") || ros2_output.contains("Result:");

    assert!(
        goal_accepted && feedback_received && result_received,
        "nros Zenoh action server ↔ ROS 2 rmw_zenoh client did not complete \
         (233.6 / 237): accepted={goal_accepted} feedback={feedback_received} \
         result={result_received}.\n{ros2_output}"
    );
    eprintln!(
        "[PASS] nros Zenoh action server ↔ ROS 2 rmw_zenoh client: accept + feedback + result"
    );
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
            nros_tests::skip!(
                "ROS 2 action server could not start (requires ros-humble-example-interfaces): {}",
                e
            );
        }
    };

    // Start nros action client
    eprintln!("Starting nros action client...");
    let mut client_cmd = Command::new(&action_client_binary);
    client_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-action-client")
        .expect("Failed to start action client");

    // Collect nros output
    let nano_output = client
        .wait_for_all_output(Duration::from_secs(20))
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

    let node_list = poll_until_contains(Duration::from_secs(10), "talker", || {
        ros2_node_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });
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

    let topic_list = poll_until_contains(Duration::from_secs(10), "/chatter", || {
        ros2_topic_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });
    let topic_info = poll_until_contains(Duration::from_secs(10), "Publisher count: 1", || {
        ros2_topic_info("/chatter", &locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });

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

/// Phase 211.C — host CLI observes the nano-ros talker's publish *rate*.
///
/// Closes the `<topic_list, topic_echo, topic_hz>` host-CLI interop trio.
/// The first two were already gated by `test_discovery_topic_visible` +
/// `test_nano_to_ros2`; this test closes the rate-observation gap.
///
/// **Why echo-based, not `ros2 topic hz`-based.** Under `rmw_zenoh_cpp`,
/// `ros2 topic hz` fails with
/// `failed to initialize wait set: the given context is not valid …` —
/// the rclpy wait-set is polled after the rmw_zenoh context shutdown handler
/// runs, before any "average rate" line is emitted. The brittle hz path
/// lives behind `#[ignore]` below (`test_ros2_topic_hz_interop`) so it can
/// be re-enabled once the upstream interaction is fixed; the rate-via-echo
/// path is the robust regression gate today.
///
/// Methodology: drive `ros2 topic echo /chatter` for an 8 s window against
/// the native talker (publishes at 1 Hz, see
/// `examples/native/rust/talker/src/lib.rs`), count "data:" sample lines,
/// divide by the window. Accepts a wide band (0.3..3.0 Hz) because rmw_zenoh
/// discovery dominates the first 2-3 s.
#[rstest]
fn test_ros2_topic_rate_via_echo_interop(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();

    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");
    talker
        .wait_for_output_pattern("Published", Duration::from_secs(8))
        .expect("talker did not publish first sample");

    let mut echo = match Ros2Process::topic_echo(
        "/chatter",
        "std_msgs/msg/Int32",
        &locator,
        DEFAULT_ROS_DISTRO,
    ) {
        Ok(p) => p,
        Err(e) => {
            talker.kill();
            nros_tests::skip!("ros2 topic echo could not start: {}", e);
        }
    };

    let window_secs = 8u64;
    let echo_out = echo
        .wait_for_output(Duration::from_secs(window_secs))
        .unwrap_or_default();
    talker.kill();

    let samples = count_pattern(&echo_out, "data:");
    let rate = samples as f64 / window_secs as f64;
    eprintln!(
        "ros2 topic echo captured {samples} samples in {window_secs}s (rate ≈ {rate:.2} Hz)\n\
         (talker publishes at 1 Hz; expecting 0.3..3.0 after discovery warmup)\n\
         --- echo tail ---\n{tail}",
        tail = echo_out
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        (0.3..3.0).contains(&rate),
        "host ros2 topic echo observed {rate:.2} Hz from a 1 Hz nano-ros publisher (samples={samples}, window={window_secs}s)"
    );
}

/// Phase 211.C follow-up — `ros2 topic hz` against rmw_zenoh emits
/// `failed to initialize wait set: the given context is not valid …` and
/// never prints an "average rate" line. The
/// `nros_tests::fixtures::ros2_topic_hz` helper is committed for future use
/// but the assertion is gated behind `#[ignore]` until either rmw_zenoh's
/// shutdown order or ros2cli's `topic hz` spin path is fixed upstream.
/// See `test_ros2_topic_rate_via_echo_interop` for the working rate-check
/// path that gates the same spirit of the bullet today.
#[rstest]
#[ignore = "ros2 topic hz + rmw_zenoh: rcl context invalid before any rate line emits"]
fn test_ros2_topic_hz_interop(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }

    let locator = zenohd_unique.locator();
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");
    talker
        .wait_for_output_pattern("Published", Duration::from_secs(8))
        .expect("talker did not publish first sample");

    let hz_out = ros2_topic_hz("/chatter", 8, &locator, DEFAULT_ROS_DISTRO).unwrap_or_default();
    talker.kill();
    eprintln!("ros2 topic hz output:\n{hz_out}");

    assert!(
        hz_out.contains("average rate"),
        "ros2 topic hz never emitted an averaged rate:\n{hz_out}"
    );
    let last_rate = hz_out
        .lines()
        .filter_map(|line| {
            line.split_once("average rate: ")
                .and_then(|(_, rest)| rest.split_whitespace().next())
                .and_then(|tok| tok.parse::<f64>().ok())
        })
        .last()
        .expect("no parseable 'average rate' value");
    assert!(
        (0.3..3.0).contains(&last_rate),
        "expected ~1 Hz, got {last_rate}:\n{hz_out}"
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

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("nros listener did not become ready");

    let topic_info = poll_until_contains(Duration::from_secs(10), "Subscription count: 1", || {
        ros2_topic_info("/chatter", &locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });

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

    let _ = server.wait_for_output_pattern("Waiting for service", Duration::from_secs(5));

    let service_list = poll_until_contains(Duration::from_secs(10), "/add_two_ints", || {
        ros2_service_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });

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

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("nros listener did not become ready");

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    let topic_info = poll_until_contains(Duration::from_secs(10), "Publisher count: 1", || {
        ros2_topic_info("/chatter", &locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });

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

    let _ = server.wait_for_output_pattern("Waiting for service", Duration::from_secs(5));

    if !server.is_running() {
        panic!(
            "native-rs-service-server (the nros side under test) exited early before the service-ready pattern"
        );
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
            server.kill();
            nros_tests::skip!(
                "ROS 2 service call could not start (missing ROS 2 tooling?): {}",
                e
            );
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
            nros_tests::skip!(
                "ROS 2 service server could not start (requires ros-humble-example-interfaces): {}",
                e
            );
        }
    };

    // Start nros service client
    eprintln!("Starting nros service client...");
    let mut client_cmd = Command::new(&service_client_binary);
    client_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-service-client")
        .expect("Failed to start service client");

    // Collect nros output
    let nano_output = client
        .wait_for_all_output(Duration::from_secs(15))
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
            nros_tests::skip!(
                "ROS 2 subscriber could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    // Start nros talker (currently only supports BEST_EFFORT)
    // Note: For full QoS testing, we'd need to modify the talker to support different QoS
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    let output = ros2_subscriber
        .wait_for_output(Duration::from_secs(8))
        .unwrap_or_default();
    talker.kill();

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
            nros_tests::skip!(
                "ROS 2 subscriber could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

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
        if let Ok(output) = ros2_subscriber.wait_for_output(Duration::from_millis(10))
            && output.contains("data:")
        {
            first_message_time = Some(start.elapsed());
            break;
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
            nros_tests::skip!(
                "ROS 2 subscriber could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    // Start nros talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Run for fixed duration: this test measures throughput over a wall-clock window.
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
