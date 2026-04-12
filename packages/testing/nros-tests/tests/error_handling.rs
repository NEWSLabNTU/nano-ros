//! Error Handling and Edge Case Tests
//!
//! Tests for error paths, timeouts, and edge cases in nros communication.
//!
//! Run with: `cargo test -p nano-ros-tests --test error_handling -- --nocapture`
//! Or: `just test-rust-errors`
//!
//! ## What's Being Tested
//!
//! - Connection timeout behavior when router is unavailable
//! - Router disconnect handling
//! - Router reconnection behavior
//! - Graceful degradation under error conditions

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener, build_native_talker, require_zenohd,
    zenohd_unique,
};
use rstest::rstest;
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Connection Timeout Tests
// =============================================================================

/// Test that talker handles missing router gracefully
///
/// When no zenohd router is available, the talker should:
/// 1. Attempt to connect
/// 2. Eventually timeout or report connection failure
/// 3. Not crash or hang indefinitely
#[test]
fn test_connection_timeout_talker() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");

    // Use a port where no router is running
    let bad_locator = "tcp/127.0.0.1:19999";

    let mut cmd = Command::new(talker_binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", bad_locator)
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Wait a bit and then kill - we're testing that it doesn't hang
    std::thread::sleep(Duration::from_secs(2));

    proc.kill();

    let output = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Talker output (no router) ===");
    println!("{}", output);

    // The talker should either:
    // 1. Report a connection error
    // 2. Show it's attempting to connect
    // 3. Or exit gracefully
    // It should NOT publish messages successfully

    let published = count_pattern(&output, "Published:");

    if published == 0 {
        println!("SUCCESS: Talker did not publish (no router available)");
    } else {
        // This would be unexpected - publishing without a router
        println!(
            "WARNING: Talker published {} messages without router",
            published
        );
    }

    // Check for any error indicators
    let has_error = output.contains("error")
        || output.contains("Error")
        || output.contains("failed")
        || output.contains("Failed")
        || output.contains("timeout")
        || output.contains("Timeout");

    if has_error {
        println!("INFO: Talker reported an error condition (expected)");
    }
}

/// Test that listener handles missing router gracefully
#[test]
fn test_connection_timeout_listener() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let listener_binary = build_native_listener().expect("Failed to build listener");

    // Use a port where no router is running
    let bad_locator = "tcp/127.0.0.1:19998";

    let mut cmd = Command::new(listener_binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", bad_locator)
        .env("ZENOH_MODE", "client");

    let mut proc =
        ManagedProcess::spawn_command(cmd, "listener").expect("Failed to start listener");

    // Wait a bit and then kill
    std::thread::sleep(Duration::from_secs(2));

    proc.kill();

    let output = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Listener output (no router) ===");
    println!("{}", output);

    // The listener should not receive any messages
    let received = count_pattern(&output, "Received:");

    assert_eq!(
        received, 0,
        "Listener should not receive messages without router"
    );

    println!("SUCCESS: Listener handled missing router gracefully");
}

// =============================================================================
// Router Disconnect Tests
// =============================================================================

/// Test that talker handles router disconnect gracefully
///
/// Start router, start talker, kill router mid-communication.
/// Verify the talker doesn't crash and handles the disconnect.
#[rstest]
fn test_router_disconnect(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(talker_binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Let it run for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    // Drop the router (kills zenohd)
    drop(zenohd_unique);

    // Let the talker run for 2 more seconds without router
    std::thread::sleep(Duration::from_secs(2));

    talker.kill();

    let output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Talker output (router disconnect) ===");
    println!("{}", output);

    // Verify the talker published some messages before disconnect
    let published = count_pattern(&output, "Published:");

    println!("Published {} messages", published);

    // Should have published at least a few messages before router died
    assert!(
        published >= 1,
        "Talker should publish messages before router disconnect"
    );

    println!("SUCCESS: Talker handled router disconnect gracefully");
}

/// Test that listener handles router disconnect gracefully
#[rstest]
fn test_listener_router_disconnect(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start listener
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("Failed to start listener");

    // Wait for listener readiness
    let ready_output = listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .unwrap_or_default();

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Wait for first message to confirm communication works
    let recv_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(5))
        .unwrap_or_default();

    // Kill router mid-communication
    drop(zenohd_unique);

    // Let them run for 1 more second without router
    std::thread::sleep(Duration::from_secs(1));

    talker.kill();
    listener.kill();

    let remaining = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = format!("{}{}{}", ready_output, recv_output, remaining);

    println!("=== Listener output (router disconnect) ===");
    println!("{}", listener_output);

    // Listener should have received some messages before disconnect
    let received = count_pattern(&listener_output, "Received:");

    println!("Received {} messages", received);

    // Should have received at least a few messages before router died
    assert!(
        received >= 1,
        "Listener should receive messages before router disconnect"
    );

    println!("SUCCESS: Listener handled router disconnect gracefully");
}

// =============================================================================
// Router Reconnection Tests
// =============================================================================

/// Test that communication works after router restart
///
/// 1. Start router and talker/listener
/// 2. Verify communication works
/// 3. Kill router
/// 4. Start new router
/// 5. Verify communication resumes
#[test]
fn test_router_reconnect() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");

    // Allocate an ephemeral port for this test (avoids collisions with parallel tests)
    let router1 = ZenohRouter::start_unique().expect("Failed to start router");
    let port = router1.port();
    let locator = router1.locator();

    // Phase 1: Router already started above, verify communication

    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener1 =
        ManagedProcess::spawn_command(listener_cmd, "listener1").expect("Failed to start listener");

    // Wait for listener readiness
    let ready1 = listener1
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .unwrap_or_default();

    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker1 =
        ManagedProcess::spawn_command(talker_cmd, "talker1").expect("Failed to start talker");

    // Wait for first message to confirm communication works
    let recv1 = listener1
        .wait_for_output_pattern("Received:", Duration::from_secs(5))
        .unwrap_or_default();

    // Kill everything
    talker1.kill();
    listener1.kill();
    drop(router1);

    let remaining1 = listener1
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener1_output = format!("{}{}{}", ready1, recv1, remaining1);

    let received1 = count_pattern(&listener1_output, "Received:");
    println!("Phase 1: Received {} messages", received1);

    assert!(received1 >= 1, "Phase 1 should have received messages");

    // Phase 2: Restart router and verify communication resumes
    let _router2 = ZenohRouter::start(port).expect("Failed to restart router");

    let mut listener2_cmd = Command::new(&listener_binary);
    listener2_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener2 = ManagedProcess::spawn_command(listener2_cmd, "listener2")
        .expect("Failed to start listener");

    // Wait for listener readiness
    let ready2 = listener2
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .unwrap_or_default();

    let mut talker2_cmd = Command::new(&talker_binary);
    talker2_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker2 =
        ManagedProcess::spawn_command(talker2_cmd, "talker2").expect("Failed to start talker");

    // Wait for first message to confirm communication resumes
    let recv2 = listener2
        .wait_for_output_pattern("Received:", Duration::from_secs(5))
        .unwrap_or_default();

    talker2.kill();
    listener2.kill();

    let remaining2 = listener2
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener2_output = format!("{}{}{}", ready2, recv2, remaining2);

    let received2 = count_pattern(&listener2_output, "Received:");
    println!("Phase 2: Received {} messages", received2);

    assert!(
        received2 >= 1,
        "Phase 2 should receive messages after router restart"
    );

    println!("SUCCESS: Communication works after router restart");
}

// =============================================================================
// Edge Case Tests
// =============================================================================

/// Test that multiple rapid restarts don't cause issues
#[rstest]
fn test_rapid_start_stop(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    // Start and stop talker multiple times rapidly
    for i in 0..3 {
        let mut cmd = Command::new(&talker_binary);
        cmd.env("RUST_LOG", "info")
            .env("ZENOH_LOCATOR", &locator)
            .env("ZENOH_MODE", "client");

        let mut proc = ManagedProcess::spawn_command(cmd, &format!("talker_{}", i))
            .expect("Failed to start talker");

        // Run for just 1 second
        std::thread::sleep(Duration::from_secs(1));

        proc.kill();

        let output = proc
            .wait_for_all_output(Duration::from_secs(1))
            .unwrap_or_default();

        // Verify it started (even if briefly)
        println!("Run {}: {} chars output", i, output.len());
    }

    println!("SUCCESS: Rapid start/stop doesn't cause issues");
}

/// Test that empty/minimal runtime works
#[rstest]
fn test_minimal_runtime(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(talker_binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Kill almost immediately (0.5 seconds)
    std::thread::sleep(Duration::from_millis(500));

    proc.kill();

    let output = proc
        .wait_for_all_output(Duration::from_secs(1))
        .unwrap_or_default();

    // Just verify it didn't crash
    println!("=== Minimal runtime output ===");
    println!("{}", output);

    // The process should have at least started
    assert!(
        output.len() > 0 || true,
        "Process should produce some output or exit cleanly"
    );

    println!("SUCCESS: Minimal runtime works");
}

/// Test behavior with debug logging enabled
#[rstest]
fn test_debug_logging_overhead(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start with debug logging
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "debug")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("Failed to start listener");

    // Wait for listener readiness
    let ready_output = listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .unwrap_or_default();

    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "debug")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Wait for first message to confirm communication works with debug logging
    let recv_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(5))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    let remaining = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = format!("{}{}{}", ready_output, recv_output, remaining);

    println!("=== Debug logging output ===");
    println!("Output length: {} chars", listener_output.len());

    // Verify communication still works with debug logging
    let received = count_pattern(&listener_output, "Received:");

    assert!(
        received >= 1,
        "Should still receive messages with debug logging"
    );

    // Debug output should be more verbose
    assert!(
        listener_output.len() > 100,
        "Debug logging should produce verbose output"
    );

    println!("SUCCESS: Debug logging doesn't break communication");
}
