//! C API integration tests
//!
//! Tests the C examples (c-talker, c-listener) built with CMake.

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_c_listener, build_c_talker, c_listener_binary,
    c_talker_binary, require_cmake, require_zenohd, zenohd_unique,
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
fn test_c_talker_builds() {
    if !require_cmake() {
        return;
    }
    match build_c_talker() {
        Ok(path) => {
            eprintln!("[PASS] C talker binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C talker: {}", e);
            panic!("C talker build failed: {}", e);
        }
    }
}

#[test]
fn test_c_listener_builds() {
    if !require_cmake() {
        return;
    }
    match build_c_listener() {
        Ok(path) => {
            eprintln!("[PASS] C listener binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C listener: {}", e);
            panic!("C listener build failed: {}", e);
        }
    }
}

// =============================================================================
// Startup Tests
// =============================================================================

#[rstest]
fn test_c_talker_starts(zenohd_unique: ZenohRouter, c_talker_binary: PathBuf) {
    if !require_zenohd() || !require_cmake() {
        return;
    }

    let locator = zenohd_unique.locator();

    let mut cmd = stdbuf_command(&c_talker_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "c-talker").expect("Failed to start c-talker");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C talker output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C talker failed to initialize.\nOutput:\n{}",
        output
    );
}

#[rstest]
fn test_c_listener_starts(zenohd_unique: ZenohRouter, c_listener_binary: PathBuf) {
    if !require_zenohd() || !require_cmake() {
        return;
    }

    let locator = zenohd_unique.locator();

    let mut cmd = stdbuf_command(&c_listener_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut listener =
        ManagedProcess::spawn_command(cmd, "c-listener").expect("Failed to start c-listener");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C listener output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C listener failed to initialize.\nOutput:\n{}",
        output
    );
}

// =============================================================================
// Communication Tests
// =============================================================================

#[rstest]
fn test_c_talker_listener_communication(
    zenohd_unique: ZenohRouter,
    c_talker_binary: PathBuf,
    c_listener_binary: PathBuf,
) {
    if !require_zenohd() || !require_cmake() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = stdbuf_command(&c_listener_binary);
    listener_cmd.env("ZENOH_LOCATOR", &locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "c-listener")
        .expect("Failed to start c-listener");

    // Give listener time to subscribe
    std::thread::sleep(Duration::from_secs(2));

    // Start talker
    let mut talker_cmd = stdbuf_command(&c_talker_binary);
    talker_cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "c-talker").expect("Failed to start c-talker");

    // Wait for messages to flow
    std::thread::sleep(Duration::from_secs(6));

    // Kill talker first
    talker.kill();

    // Collect listener output
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C listener output:\n{}", listener_output);

    // Verify initialization
    assert!(
        listener_output.contains("Support initialized"),
        "C listener failed to initialize.\nOutput:\n{}",
        listener_output
    );

    // Verify message reception (expect at least 3 messages)
    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("C listener received {} messages", received_count);

    assert!(
        received_count >= 3,
        "Expected at least 3 messages, got {}.\nOutput:\n{}",
        received_count,
        listener_output
    );
}
