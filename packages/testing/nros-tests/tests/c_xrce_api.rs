//! C XRCE-DDS API integration tests
//!
//! Tests the C examples (c-xrce-talker, c-xrce-listener) built with CMake
//! using the XRCE-DDS backend.
//!
//! Prerequisites:
//!   just build-xrce-agent   # Build the Micro-XRCE-DDS Agent from source
//!   cmake                   # Required for building C examples

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, XrceAgent, build_c_xrce_listener, build_c_xrce_talker, c_xrce_listener_binary,
    c_xrce_talker_binary, require_cmake, require_xrce_agent,
};
use rstest::rstest;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Create a Command that wraps a C binary with `stdbuf -oL -eL` to force
/// line-buffered stdout/stderr. C's printf fully-buffers when piped.
fn stdbuf_command(binary: &Path) -> Command {
    let mut cmd = Command::new("stdbuf");
    cmd.args(["-oL", "-eL"]).arg(binary);
    cmd
}

// =============================================================================
// Build Tests
// =============================================================================

#[test]
fn test_c_xrce_talker_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    match build_c_xrce_talker() {
        Ok(path) => {
            eprintln!("[PASS] C XRCE talker binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C XRCE talker: {}", e);
            panic!("C XRCE talker build failed: {}", e);
        }
    }
}

#[test]
fn test_c_xrce_listener_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    match build_c_xrce_listener() {
        Ok(path) => {
            eprintln!("[PASS] C XRCE listener binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C XRCE listener: {}", e);
            panic!("C XRCE listener build failed: {}", e);
        }
    }
}

// =============================================================================
// Startup Tests
// =============================================================================

#[rstest]
fn test_c_xrce_talker_starts(c_xrce_talker_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = stdbuf_command(&c_xrce_talker_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "c-xrce-talker").expect("Failed to start c-xrce-talker");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C XRCE talker output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C XRCE talker failed to initialize.\nOutput:\n{}",
        output
    );
}

#[rstest]
fn test_c_xrce_listener_starts(c_xrce_listener_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = stdbuf_command(&c_xrce_listener_binary);
    cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut listener = ManagedProcess::spawn_command(cmd, "c-xrce-listener")
        .expect("Failed to start c-xrce-listener");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C XRCE listener output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C XRCE listener failed to initialize.\nOutput:\n{}",
        output
    );
}

// =============================================================================
// Communication Tests
// =============================================================================

#[rstest]
fn test_c_xrce_talker_listener_communication(
    c_xrce_talker_binary: PathBuf,
    c_xrce_listener_binary: PathBuf,
) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start listener first (subscribe before publishing)
    let mut listener_cmd = stdbuf_command(&c_xrce_listener_binary);
    listener_cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "c-xrce-listener")
        .expect("Failed to start c-xrce-listener");

    // Give listener time to subscribe
    std::thread::sleep(Duration::from_secs(3));

    // Start talker
    let mut talker_cmd = stdbuf_command(&c_xrce_talker_binary);
    talker_cmd.env("XRCE_AGENT_ADDR", &addr);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "c-xrce-talker")
        .expect("Failed to start c-xrce-talker");

    // Wait for messages to flow
    std::thread::sleep(Duration::from_secs(8));

    // Kill talker first
    talker.kill();

    // Collect listener output
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C XRCE listener output:\n{}", listener_output);

    // Verify initialization
    assert!(
        listener_output.contains("Support initialized"),
        "C XRCE listener failed to initialize.\nOutput:\n{}",
        listener_output
    );

    // Verify message reception (expect at least 3 messages)
    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("C XRCE listener received {} messages", received_count);

    assert!(
        received_count >= 3,
        "Expected at least 3 messages, got {}.\nOutput:\n{}",
        received_count,
        listener_output
    );
}
