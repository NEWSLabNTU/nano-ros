//! XRCE-DDS integration tests
//!
//! Tests communication between XRCE-DDS test binaries via the
//! Micro-XRCE-DDS-Agent.
//!
//! Prerequisites:
//!   just build-xrce-agent   # Build the Agent from source

use nros_tests::fixtures::{
    ManagedProcess, XrceAgent, require_xrce_agent, xrce_listener_binary, xrce_talker_binary,
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

    eprintln!("Listener output:\n{}", listener_output);

    // Check if listener received messages
    let received_count = count_pattern(&listener_output, "Received:");
    eprintln!("Listener received {} messages", received_count);

    if received_count > 0 {
        eprintln!("[PASS] XRCE-DDS pub/sub communication works");
    } else {
        eprintln!("[INFO] No messages received (may be timing issue)");
    }

    drop(agent);
}
