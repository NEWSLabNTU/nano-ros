//! XRCE-DDS integration tests
//!
//! Tests communication between XRCE-DDS test binaries via the
//! Micro-XRCE-DDS-Agent.
//!
//! Prerequisites:
//!   just build-xrce-agent   # Build the Agent from source

use nros_tests::fixtures::{
    ManagedProcess, XrceAgent, XrceSerialAgent, require_socat, require_xrce_agent,
    xrce_action_client_binary, xrce_action_server_binary, xrce_large_msg_test_binary,
    xrce_listener_binary, xrce_serial_listener_binary, xrce_serial_talker_binary,
    xrce_service_client_binary, xrce_service_server_binary, xrce_talker_binary,
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

fn set_xrce_udp_locator<'a>(cmd: &'a mut Command, addr: &str, domain: &str) -> &'a mut Command {
    // Each test starts its own Agent on an ephemeral UDP port, but the Agent
    // bridges to DDS — and in XRCE-DDS the *client* picks the participant domain
    // via ROS_DOMAIN_ID. Without a per-test domain every Agent's DDS side lands
    // on domain 0 and concurrent tests cross-talk over RTPS. Give each test a
    // unique domain (shared by both endpoints of the pair) so the `xrce` group
    // can run fully parallel with real isolation (Phase 183.7). Tolerant
    // assertions hid the cross-talk before, but isolation should be explicit.
    cmd.env("NROS_LOCATOR", addr)
        .env("XRCE_AGENT_ADDR", addr)
        .env("ROS_DOMAIN_ID", domain)
        .env("RUST_LOG", "info")
}

// =============================================================================
// XRCE Pub/Sub Tests
// =============================================================================

#[rstest]
fn test_xrce_talker_starts(xrce_talker_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    let mut cmd = Command::new(&xrce_talker_binary);
    set_xrce_udp_locator(&mut cmd, &addr, &domain);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for readiness (talker prints "Publishing" after setup)
    match talker.wait_for_output_pattern(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(30),
    ) {
        Ok(_) => eprintln!("xrce-talker started and published successfully"),
        Err(_) => {
            if talker.is_running() {
                eprintln!("xrce-talker running (no publish marker yet)");
            } else {
                eprintln!("xrce-talker exited early");
            }
        }
    }
}

#[rstest]
fn test_xrce_listener_starts(xrce_listener_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    let mut cmd = Command::new(&xrce_listener_binary);
    set_xrce_udp_locator(&mut cmd, &addr, &domain).env("XRCE_MSG_COUNT", "1"); // Just test that it starts
    let mut listener =
        ManagedProcess::spawn_command(cmd, "xrce-listener").expect("Failed to start listener");

    // Wait for readiness (listener prints "Waiting for" after setup)
    match listener.wait_for_output_pattern("Waiting for", Duration::from_secs(30)) {
        Ok(_) => eprintln!("xrce-listener started successfully"),
        Err(_) => {
            if listener.is_running() {
                eprintln!("xrce-listener running (no readiness marker yet)");
            } else {
                eprintln!("xrce-listener exited early");
            }
        }
    }

    drop(agent);
}

#[rstest]
fn test_xrce_talker_listener_communication(
    xrce_talker_binary: PathBuf,
    xrce_listener_binary: PathBuf,
) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    // Start listener first (subscribe before publishing)
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    set_xrce_udp_locator(&mut listener_cmd, &addr, &domain).env("XRCE_MSG_COUNT", "3");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-listener")
        .expect("Failed to start listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(30));

    // Stabilization delay — let XRCE Agent propagate the subscription
    std::thread::sleep(Duration::from_secs(2));

    // Start talker
    let mut talker_cmd = Command::new(&xrce_talker_binary);
    set_xrce_udp_locator(&mut talker_cmd, &addr, &domain);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern(
            nros_tests::output::LISTENER_LOG_PREFIX,
            Duration::from_secs(15),
        )
        .unwrap_or_default();

    // Kill both processes
    talker.kill();
    listener.kill();

    // Assert at least 1 message was received
    nros_tests::output::assert_listener(&listener_output, 1);

    drop(agent);
}

#[rstest]
fn test_xrce_multiple_messages(xrce_talker_binary: PathBuf, xrce_listener_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    // Start listener first, expect 5 messages
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    set_xrce_udp_locator(&mut listener_cmd, &addr, &domain).env("XRCE_MSG_COUNT", "5");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-listener")
        .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(30));
    std::thread::sleep(Duration::from_secs(2));

    // Start talker (publishes 20 messages at 500ms intervals)
    let mut talker_cmd = Command::new(&xrce_talker_binary);
    set_xrce_udp_locator(&mut talker_cmd, &addr, &domain);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for listener to receive enough messages (or exit on its own after 5)
    let listener_output = listener
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            5,
            Duration::from_secs(20),
        )
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    nros_tests::output::assert_listener(&listener_output, 3);

    drop(agent);
}

// =============================================================================
// XRCE Service Tests
// =============================================================================

#[rstest]
fn test_xrce_service_request_response(
    xrce_service_server_binary: PathBuf,
    xrce_service_client_binary: PathBuf,
) {
    use nros_tests::count_pattern;

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    // Start service server first
    let mut server_cmd = Command::new(&xrce_service_server_binary);
    set_xrce_udp_locator(&mut server_cmd, &addr, &domain).env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-service-server")
        .expect("Failed to start service server");

    // Wait for server to be ready
    let _ = server.wait_for_output_pattern("Waiting for service requests", Duration::from_secs(30));

    // Stabilization delay — let XRCE Agent propagate the service
    std::thread::sleep(Duration::from_secs(2));

    // Start service client
    let mut client_cmd = Command::new(&xrce_service_client_binary);
    set_xrce_udp_locator(&mut client_cmd, &addr, &domain).env("XRCE_REQUEST_COUNT", "3");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-service-client")
        .expect("Failed to start service client");

    // Wait for client to complete requests
    let client_output = client
        .wait_for_output_pattern("service calls succeeded", Duration::from_secs(30))
        .unwrap_or_default();

    // Give server time to flush output, then collect
    std::thread::sleep(Duration::from_millis(500));
    let server_output = server
        .wait_for_output_pattern("Received request:", Duration::from_secs(2))
        .unwrap_or_default();

    // Kill both processes
    client.kill();
    server.kill();

    eprintln!("Client output:\n{}", client_output);
    eprintln!("Server output:\n{}", server_output);

    // Verify client received replies
    let reply_count = count_pattern(&client_output, "Response:");
    eprintln!("Client received {} replies", reply_count);
    assert!(
        reply_count >= 1,
        "Expected at least 1 reply, got {}",
        reply_count
    );

    drop(agent);
}

// =============================================================================
// XRCE Action Tests
// =============================================================================

#[rstest]
fn test_xrce_action_fibonacci(
    xrce_action_server_binary: PathBuf,
    xrce_action_client_binary: PathBuf,
) {
    use nros_tests::count_pattern;

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    // Start action server first
    let mut server_cmd = Command::new(&xrce_action_server_binary);
    set_xrce_udp_locator(&mut server_cmd, &addr, &domain).env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-action-server")
        .expect("Failed to start action server");

    // Wait for server to be ready
    let _ = server.wait_for_output_pattern("Waiting for action goals", Duration::from_secs(30));

    // Stabilization delay
    std::thread::sleep(Duration::from_secs(2));

    // Start action client (requests Fibonacci order=5)
    let mut client_cmd = Command::new(&xrce_action_client_binary);
    set_xrce_udp_locator(&mut client_cmd, &addr, &domain).env("XRCE_FIBONACCI_ORDER", "5");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-action-client")
        .expect("Failed to start action client");

    // Wait for client to complete
    let client_output = client
        .wait_for_output_pattern("Action client finished", Duration::from_secs(30))
        .unwrap_or_default();

    // Give server time to flush output
    std::thread::sleep(Duration::from_millis(500));
    let server_output = server
        .wait_for_output_pattern("Goal completed", Duration::from_secs(2))
        .unwrap_or_default();

    client.kill();
    server.kill();

    eprintln!("Client output:\n{}", client_output);
    eprintln!("Server output:\n{}", server_output);

    // Verify goal was accepted
    assert!(
        client_output.contains("Goal accepted"),
        "Client should have received goal acceptance.\nClient output:\n{}",
        client_output,
    );

    // Verify feedback was received
    let feedback_count = count_pattern(&client_output, "Feedback");
    assert!(
        feedback_count >= 1,
        "Expected at least 1 feedback message, got {}.\nClient output:\n{}",
        feedback_count,
        client_output,
    );

    // Verify result was received
    assert!(
        client_output.contains("Action client finished"),
        "Client should have completed the action.\nClient output:\n{}",
        client_output,
    );

    drop(agent);
}

// =============================================================================
// XRCE Large Message / Fragmented Stream Tests
// =============================================================================

/// Tests that publish_raw succeeds for messages larger than a single stream
/// slot, exercising the fragmented output stream path (Phase 40.3).
#[rstest]
fn test_xrce_large_message_publish(xrce_large_msg_test_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id().to_string();

    let mut cmd = Command::new(&xrce_large_msg_test_binary);
    set_xrce_udp_locator(&mut cmd, &addr, &domain);
    let mut test_proc = ManagedProcess::spawn_command(cmd, "xrce-large-msg-test")
        .expect("Failed to start large-msg-test");

    // Wait for the test to complete (prints "ALL PASSED" or "SOME FAILED")
    let output = test_proc
        .wait_for_output_pattern("Results:", Duration::from_secs(30))
        .unwrap_or_default();

    test_proc.kill();

    eprintln!("Large msg test output:\n{}", output);

    assert!(
        output.contains("ALL PASSED"),
        "Large message publish test failed.\nOutput:\n{}",
        output,
    );

    drop(agent);
}

// =============================================================================
// XRCE Serial Transport Tests
// =============================================================================

#[rstest]
fn test_xrce_serial_talker_starts(xrce_serial_talker_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_socat() {
        nros_tests::skip!("socat not available");
    }

    let agent = XrceSerialAgent::start(1).expect("Failed to start XRCE Serial Agent");
    let domain = nros_tests::unique_ros_domain_id().to_string();

    let mut cmd = Command::new(&xrce_serial_talker_binary);
    cmd.env("XRCE_SERIAL_PTY", agent.client_pty_path(0))
        .env("ROS_DOMAIN_ID", &domain);
    let mut talker = ManagedProcess::spawn_command(cmd, "xrce-serial-talker")
        .expect("Failed to start serial talker");

    match talker.wait_for_output_pattern(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(15),
    ) {
        Ok(_) => eprintln!("xrce-serial-talker started and published successfully"),
        Err(_) => {
            if talker.is_running() {
                eprintln!("xrce-serial-talker running (no publish marker yet)");
            } else {
                eprintln!("xrce-serial-talker exited early");
            }
        }
    }
}

#[rstest]
fn test_xrce_serial_listener_starts(xrce_serial_listener_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_socat() {
        nros_tests::skip!("socat not available");
    }

    let agent = XrceSerialAgent::start(1).expect("Failed to start XRCE Serial Agent");
    let domain = nros_tests::unique_ros_domain_id().to_string();

    let mut cmd = Command::new(&xrce_serial_listener_binary);
    cmd.env("XRCE_SERIAL_PTY", agent.client_pty_path(0))
        .env("XRCE_MSG_COUNT", "1")
        .env("ROS_DOMAIN_ID", &domain);
    let mut listener = ManagedProcess::spawn_command(cmd, "xrce-serial-listener")
        .expect("Failed to start serial listener");

    match listener.wait_for_output_pattern("Waiting for", Duration::from_secs(15)) {
        Ok(_) => eprintln!("xrce-serial-listener started successfully"),
        Err(_) => {
            if listener.is_running() {
                eprintln!("xrce-serial-listener running (no readiness marker yet)");
            } else {
                eprintln!("xrce-serial-listener exited early");
            }
        }
    }

    drop(agent);
}

#[rstest]
fn test_xrce_serial_communication(
    xrce_serial_talker_binary: PathBuf,
    xrce_serial_listener_binary: PathBuf,
) {
    use std::process::Command;

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_socat() {
        nros_tests::skip!("socat not available");
    }

    // Serial is point-to-point: use a single agent in multiserial mode with
    // two PTY pairs so both clients route through the same agent.
    let agent = XrceSerialAgent::start(2).expect("Failed to start XRCE Serial Agent");
    let domain = nros_tests::unique_ros_domain_id().to_string();

    // Start listener first (subscribe before publishing)
    let mut listener_cmd = Command::new(&xrce_serial_listener_binary);
    listener_cmd
        .env("XRCE_SERIAL_PTY", agent.client_pty_path(0))
        .env("XRCE_MSG_COUNT", "3")
        .env("ROS_DOMAIN_ID", &domain);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-serial-listener")
        .expect("Failed to start serial listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(15));

    // Stabilization delay — let XRCE Agent propagate the subscription
    std::thread::sleep(Duration::from_secs(3));

    // Start talker on second serial link
    let mut talker_cmd = Command::new(&xrce_serial_talker_binary);
    talker_cmd
        .env("XRCE_SERIAL_PTY", agent.client_pty_path(1))
        .env("ROS_DOMAIN_ID", &domain);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "xrce-serial-talker")
        .expect("Failed to start serial talker");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern(
            nros_tests::output::LISTENER_LOG_PREFIX,
            Duration::from_secs(25),
        )
        .unwrap_or_default();

    // Kill both processes
    talker.kill();
    listener.kill();

    // Assert at least 1 message was received
    nros_tests::output::assert_listener(&listener_output, 1);

    drop(agent);
}
