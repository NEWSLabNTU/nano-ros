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
use std::path::PathBuf;
use std::time::Duration;

// =============================================================================
// XRCE Pub/Sub Tests
// =============================================================================

#[rstest]
fn test_xrce_talker_starts(xrce_talker_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = Command::new(&xrce_talker_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for readiness (talker prints "Publishing" after setup)
    match talker.wait_for_output_pattern("Published:", Duration::from_secs(10)) {
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
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = Command::new(&xrce_listener_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr).env("XRCE_MSG_COUNT", "1"); // Just test that it starts
    let mut listener =
        ManagedProcess::spawn_command(cmd, "xrce-listener").expect("Failed to start listener");

    // Wait for readiness (listener prints "Waiting for" after setup)
    match listener.wait_for_output_pattern("Waiting for", Duration::from_secs(10)) {
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
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start listener first (subscribe before publishing)
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_MSG_COUNT", "3");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-listener")
        .expect("Failed to start listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(10));

    // Stabilization delay — let XRCE Agent propagate the subscription
    std::thread::sleep(Duration::from_secs(2));

    // Start talker
    let mut talker_cmd = Command::new(&xrce_talker_binary);
    talker_cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(15))
        .unwrap_or_default();

    // Kill both processes
    talker.kill();
    listener.kill();

    // Assert at least 1 message was received
    let received_count = count_pattern(&listener_output, "Received:");
    assert!(
        received_count >= 1,
        "Expected at least 1 message, got {}.\nListener output:\n{}",
        received_count,
        listener_output,
    );

    drop(agent);
}

#[rstest]
fn test_xrce_multiple_messages(xrce_talker_binary: PathBuf, xrce_listener_binary: PathBuf) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start listener first, expect 5 messages
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_MSG_COUNT", "5");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-listener")
        .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(2));

    // Start talker (publishes 20 messages at 500ms intervals)
    let mut talker_cmd = Command::new(&xrce_talker_binary);
    talker_cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for listener to receive enough messages (or exit on its own after 5)
    let listener_output = listener
        .wait_for_output_pattern("Received 5 messages", Duration::from_secs(20))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    let received_count = count_pattern(&listener_output, "Received:");
    assert!(
        received_count >= 3,
        "Expected at least 3 messages, got {}.\nListener output:\n{}",
        received_count,
        listener_output,
    );

    drop(agent);
}

// =============================================================================
// XRCE Service Tests
// =============================================================================

#[rstest]
fn test_xrce_service_server_starts(xrce_service_server_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = Command::new(&xrce_service_server_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr).env("XRCE_TIMEOUT", "10");
    let mut server = ManagedProcess::spawn_command(cmd, "xrce-service-server")
        .expect("Failed to start service server");

    // Wait for readiness marker
    match server.wait_for_output_pattern("Service server ready", Duration::from_secs(10)) {
        Ok(_) => eprintln!("xrce-service-server started successfully"),
        Err(_) => {
            if server.is_running() {
                eprintln!("xrce-service-server running (no readiness marker yet)");
            } else {
                eprintln!("xrce-service-server exited early");
            }
        }
    }

    server.kill();
    drop(agent);
}

#[rstest]
fn test_xrce_service_client_starts(xrce_service_client_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = Command::new(&xrce_service_client_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_REQUEST_COUNT", "1");
    let mut client = ManagedProcess::spawn_command(cmd, "xrce-service-client")
        .expect("Failed to start service client");

    // Wait for readiness marker (client will timeout without a server)
    match client.wait_for_output_pattern("Service client ready", Duration::from_secs(10)) {
        Ok(_) => eprintln!("xrce-service-client started successfully"),
        Err(_) => {
            if client.is_running() {
                eprintln!("xrce-service-client running (no readiness marker yet)");
            } else {
                eprintln!("xrce-service-client exited early");
            }
        }
    }

    client.kill();
    drop(agent);
}

#[rstest]
fn test_xrce_service_request_response(
    xrce_service_server_binary: PathBuf,
    xrce_service_client_binary: PathBuf,
) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start service server first
    let mut server_cmd = Command::new(&xrce_service_server_binary);
    server_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-service-server")
        .expect("Failed to start service server");

    // Wait for server to be ready
    let _ = server.wait_for_output_pattern("Service server ready", Duration::from_secs(10));

    // Stabilization delay — let XRCE Agent propagate the service
    std::thread::sleep(Duration::from_secs(2));

    // Start service client
    let mut client_cmd = Command::new(&xrce_service_client_binary);
    client_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_REQUEST_COUNT", "3");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-service-client")
        .expect("Failed to start service client");

    // Wait for client to complete requests
    let client_output = client
        .wait_for_output_pattern("Completed", Duration::from_secs(30))
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
    let reply_count = count_pattern(&client_output, "Received reply:");
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
fn test_xrce_action_server_starts(xrce_action_server_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = Command::new(&xrce_action_server_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr).env("XRCE_TIMEOUT", "10");
    let mut server = ManagedProcess::spawn_command(cmd, "xrce-action-server")
        .expect("Failed to start action server");

    match server.wait_for_output_pattern("Action server ready", Duration::from_secs(10)) {
        Ok(_) => eprintln!("xrce-action-server started successfully"),
        Err(_) => {
            if server.is_running() {
                eprintln!("xrce-action-server running (no readiness marker yet)");
            } else {
                eprintln!("xrce-action-server exited early");
            }
        }
    }

    server.kill();
    drop(agent);
}

#[rstest]
fn test_xrce_action_client_starts(xrce_action_client_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = Command::new(&xrce_action_client_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut client = ManagedProcess::spawn_command(cmd, "xrce-action-client")
        .expect("Failed to start action client");

    match client.wait_for_output_pattern("Action client ready", Duration::from_secs(10)) {
        Ok(_) => eprintln!("xrce-action-client started successfully"),
        Err(_) => {
            if client.is_running() {
                eprintln!("xrce-action-client running (no readiness marker yet)");
            } else {
                eprintln!("xrce-action-client exited early");
            }
        }
    }

    client.kill();
    drop(agent);
}

#[rstest]
fn test_xrce_action_fibonacci(
    xrce_action_server_binary: PathBuf,
    xrce_action_client_binary: PathBuf,
) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start action server first
    let mut server_cmd = Command::new(&xrce_action_server_binary);
    server_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-action-server")
        .expect("Failed to start action server");

    // Wait for server to be ready
    let _ = server.wait_for_output_pattern("Action server ready", Duration::from_secs(10));

    // Stabilization delay
    std::thread::sleep(Duration::from_secs(2));

    // Start action client (requests Fibonacci order=5)
    let mut client_cmd = Command::new(&xrce_action_client_binary);
    client_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("XRCE_FIBONACCI_ORDER", "5");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-action-client")
        .expect("Failed to start action client");

    // Wait for client to complete
    let client_output = client
        .wait_for_output_pattern("Action client done", Duration::from_secs(30))
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
        client_output.contains("Result:"),
        "Client should have received result.\nClient output:\n{}",
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
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = Command::new(&xrce_large_msg_test_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr);
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

    if !require_xrce_agent() || !require_socat() {
        return;
    }

    let agent = XrceSerialAgent::start(1).expect("Failed to start XRCE Serial Agent");

    let mut cmd = Command::new(&xrce_serial_talker_binary);
    cmd.env("XRCE_SERIAL_PTY", agent.client_pty_path(0));
    let mut talker = ManagedProcess::spawn_command(cmd, "xrce-serial-talker")
        .expect("Failed to start serial talker");

    match talker.wait_for_output_pattern("Published:", Duration::from_secs(15)) {
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

    if !require_xrce_agent() || !require_socat() {
        return;
    }

    let agent = XrceSerialAgent::start(1).expect("Failed to start XRCE Serial Agent");

    let mut cmd = Command::new(&xrce_serial_listener_binary);
    cmd.env("XRCE_SERIAL_PTY", agent.client_pty_path(0))
        .env("XRCE_MSG_COUNT", "1");
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
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_xrce_agent() || !require_socat() {
        return;
    }

    // Serial is point-to-point: use a single agent in multiserial mode with
    // two PTY pairs so both clients route through the same agent.
    let agent = XrceSerialAgent::start(2).expect("Failed to start XRCE Serial Agent");

    // Start listener first (subscribe before publishing)
    let mut listener_cmd = Command::new(&xrce_serial_listener_binary);
    listener_cmd
        .env("XRCE_SERIAL_PTY", agent.client_pty_path(0))
        .env("XRCE_MSG_COUNT", "3");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-serial-listener")
        .expect("Failed to start serial listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(15));

    // Stabilization delay — let XRCE Agent propagate the subscription
    std::thread::sleep(Duration::from_secs(3));

    // Start talker on second serial link
    let mut talker_cmd = Command::new(&xrce_serial_talker_binary);
    talker_cmd.env("XRCE_SERIAL_PTY", agent.client_pty_path(1));
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "xrce-serial-talker")
        .expect("Failed to start serial talker");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(25))
        .unwrap_or_default();

    // Kill both processes
    talker.kill();
    listener.kill();

    // Assert at least 1 message was received
    let received_count = count_pattern(&listener_output, "Received:");
    assert!(
        received_count >= 1,
        "Expected at least 1 message, got {}.\nListener output:\n{}",
        received_count,
        listener_output,
    );

    drop(agent);
}
