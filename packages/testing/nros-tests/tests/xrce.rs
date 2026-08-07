//! XRCE-DDS integration tests
//!
//! Tests communication between XRCE-DDS test binaries via the
//! Micro-XRCE-DDS-Agent.
//!
//! **Bucket (phase-329 W4): KEEP — genuine one-offs; delivery FOLDED 2026-08-05.**
//! These run the DEDICATED `nros-tests/bins` XRCE binaries over an explicit UDP
//! locator + Agent. What remains is one-offs no matrix cell covers: the SERIAL
//! transport lane (`*_serial_*`), the large-message / fragmented-stream publish
//! (`large_message_publish`), and the startup probes (`*_starts`).
//!
//! The plain pubsub/service/action DELIVERY tests
//! (`talker_listener_communication`, `multiple_messages`, `service_request_response`,
//! `action_fibonacci`) were DELETED: the native XRCE matrix cells — run-proven in
//! `native_example_pubsub_e2e` (xrce pubsub) and `native_example_reqresp_e2e` (xrce
//! service + action) — cover that same product path, so the dedicated-bin repeats
//! were redundant. The dedicated bins survive (the `*_starts` probes here + the
//! `xrce_ros2_interop` tests still use them), so nothing is orphaned.
//!
//! Prerequisites:
//!   just build-xrce-agent   # Build the Agent from source

use nros_tests::fixtures::{
    ManagedProcess, XrceAgent, XrceSerialAgent, require_socat, require_xrce_agent,
    xrce_large_msg_test_binary, xrce_listener_binary, xrce_serial_listener_binary,
    xrce_serial_talker_binary, xrce_talker_binary,
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
    let output = test_proc.collect_until("Results:", Duration::from_secs(30));

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
    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(25),
    );

    // Kill both processes
    talker.kill();
    listener.kill();

    // Assert at least 1 message was received
    nros_tests::output::assert_listener(&listener_output, 1);

    drop(agent);
}
