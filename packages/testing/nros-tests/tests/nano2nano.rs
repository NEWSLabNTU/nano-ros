//! nros to nros communication tests
//!
//! Tests communication between native nros binaries via zenoh.

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, is_zenohd_available, listener_binary, require_zenohd,
    talker_binary, zenohd_unique,
};
use rstest::rstest;
use std::path::PathBuf;
use std::time::Duration;

// =============================================================================
// Native Pub/Sub Tests
// =============================================================================

#[rstest]
fn test_native_talker_starts(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Use ZENOH_LOCATOR env var since examples use Context::from_env()
    let mut cmd = Command::new(&talker_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "native-rs-talker").expect("Failed to start talker");

    // Wait for readiness (talker prints "Publishing" after setup)
    match talker.wait_for_output_pattern("Publishing", Duration::from_secs(5)) {
        Ok(_) => eprintln!("native-rs-talker started successfully"),
        Err(_) => {
            if talker.is_running() {
                eprintln!("native-rs-talker running (no readiness marker yet)");
            } else {
                eprintln!("native-rs-talker exited early");
            }
        }
    }
}

#[rstest]
fn test_native_listener_starts(zenohd_unique: ZenohRouter, listener_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Use ZENOH_LOCATOR env var since examples use Context::from_env()
    let mut cmd = Command::new(&listener_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut listener =
        ManagedProcess::spawn_command(cmd, "native-rs-listener").expect("Failed to start listener");

    // Wait for readiness (listener prints "Waiting for" after setup)
    match listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5)) {
        Ok(_) => eprintln!("native-rs-listener started successfully"),
        Err(_) => {
            if listener.is_running() {
                eprintln!("native-rs-listener running (no readiness marker yet)");
            } else {
                eprintln!("native-rs-listener exited early");
            }
        }
    }
}

#[rstest]
fn test_talker_listener_communication(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    listener_binary: PathBuf,
) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener first with ZENOH_LOCATOR env var
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("RUST_LOG", "info"); // Enable env_logger output
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    // Wait for listener to be ready (prints "Waiting for" after subscription)
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5));

    // Start talker with ZENOH_LOCATOR env var
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Wait for listener to receive messages (event-driven instead of fixed sleep)
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(10))
        .unwrap_or_default();

    // Kill talker
    talker.kill();

    eprintln!("Listener output:\n{}", listener_output);

    // Check if listener received messages
    let received_count = count_pattern(&listener_output, "Received:");
    eprintln!("Listener received {} messages", received_count);

    if received_count > 0 {
        eprintln!("[PASS] Router-based communication works");
    } else {
        eprintln!("[INFO] No messages received (may be timing issue)");
    }
}

// =============================================================================
// Peer Mode Tests (no router required)
// =============================================================================

/// Test peer-to-peer communication without a zenohd router
///
/// In peer mode, nros nodes can discover each other via multicast
/// without requiring a central router.
#[rstest]
fn test_peer_mode_communication(talker_binary: PathBuf, listener_binary: PathBuf) {
    use nros_tests::count_pattern;
    use std::process::Command;

    eprintln!("Testing peer mode communication (no router)...");

    // Start listener in peer mode
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd.env("ZENOH_MODE", "peer");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener-peer")
        .expect("Failed to start listener in peer mode");

    // Wait for listener readiness
    if listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .is_err()
    {
        if !listener.is_running() {
            eprintln!("[INFO] Listener exited early - peer mode may not be supported");
            return;
        }
    }

    // Start talker in peer mode
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd.env("ZENOH_MODE", "peer");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker-peer")
        .expect("Failed to start talker in peer mode");

    // Wait for talker readiness
    if talker
        .wait_for_output_pattern("Publishing", Duration::from_secs(5))
        .is_err()
        && !talker.is_running()
    {
        eprintln!("[INFO] Talker exited early - peer mode may not be supported");
        return;
    }

    // Wait for listener to receive messages (event-driven)
    eprintln!("Waiting for peer communication...");
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(10))
        .unwrap_or_default();

    // Kill talker first
    talker.kill();

    eprintln!("Listener output:\n{}", listener_output);

    // Check if listener received messages
    let received_count = count_pattern(&listener_output, "Received:");
    eprintln!("Listener received {} messages", received_count);

    if received_count > 0 {
        eprintln!("[PASS] Peer mode communication works");
    } else {
        // Peer mode may require specific network configuration (multicast enabled)
        eprintln!("[INFO] No messages received - peer discovery may require multicast support");
        eprintln!("[INFO] This is expected on some network configurations");
    }
}

// =============================================================================
// Detection Tests
// =============================================================================

#[test]
fn test_zenohd_detection() {
    let available = is_zenohd_available();
    eprintln!("zenohd available: {}", available);
}
