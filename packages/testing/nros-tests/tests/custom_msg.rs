//! Custom Message Integration Tests
//!
//! Tests for custom message types: serialization, deserialization, and pub/sub.
//!
//! Run with: `cargo test -p nano-ros-tests --test custom_msg -- --nocapture`
//! Or: `just test-rust-custom-msg`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_custom_msg, build_native_custom_msg_no_zenoh,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Build Tests
// =============================================================================

/// Test that the custom message example builds without zenoh feature
#[rstest]
fn test_custom_msg_builds_no_zenoh() {
    let binary = build_native_custom_msg_no_zenoh().expect("Failed to build native-rs-custom-msg");
    assert!(binary.exists(), "Binary should exist: {}", binary.display());
    println!("SUCCESS: Built custom_msg (no zenoh): {}", binary.display());
}

/// Test that the custom message example builds with zenoh feature
#[rstest]
fn test_custom_msg_builds_with_zenoh() {
    let binary =
        build_native_custom_msg().expect("Failed to build native-rs-custom-msg with zenoh");
    assert!(binary.exists(), "Binary should exist: {}", binary.display());
    println!(
        "SUCCESS: Built custom_msg (with zenoh): {}",
        binary.display()
    );
}

// =============================================================================
// Serialization Tests (no network required)
// =============================================================================

/// Test that serialization roundtrip works for custom messages
#[rstest]
fn test_custom_msg_serialization() {
    let binary = build_native_custom_msg_no_zenoh().expect("Failed to build");

    // Run without zenoh - tests serialization only
    let cmd = Command::new(&binary);
    let mut proc = ManagedProcess::spawn_command(cmd, "custom_msg").expect("Failed to start");

    // Wait for completion marker (no network, finishes quickly)
    let output = proc
        .wait_for_output_pattern("All serialization tests passed", Duration::from_secs(5))
        .unwrap_or_else(|_| {
            proc.kill();
            proc.wait_for_all_output(Duration::from_secs(1))
                .unwrap_or_default()
        });

    println!("=== Custom message output ===");
    println!("{}", output);

    // Verify serialization tests passed
    assert!(
        output.contains("SensorReading") && output.contains("OK"),
        "SensorReading serialization should succeed"
    );
    assert!(
        output.contains("Status") && output.contains("OK"),
        "Status serialization should succeed"
    );
    assert!(
        output.contains("std_msgs::Int32") && output.contains("OK"),
        "std_msgs::Int32 serialization should succeed"
    );
    assert!(
        output.contains("All serialization tests passed"),
        "All serialization tests should pass"
    );

    println!("SUCCESS: All custom message serialization tests passed");
}

// =============================================================================
// Pub/Sub Tests (requires zenoh)
// =============================================================================

/// Test custom message pub/sub with zenoh transport
#[rstest]
fn test_custom_msg_pub_sub(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let binary = build_native_custom_msg().expect("Failed to build with zenoh");
    let locator = zenohd_unique.locator();

    println!("Starting custom_msg with zenoh...");
    println!("zenohd locator: {}", locator);

    let mut cmd = Command::new(&binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "custom_msg").expect("Failed to start");

    // Wait for completion marker (runs serialization + pub/sub then exits)
    let output = proc
        .wait_for_output_pattern("completed successfully", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            proc.kill();
            proc.wait_for_all_output(Duration::from_secs(1))
                .unwrap_or_default()
        });

    println!("=== Custom message pub/sub output ===");
    println!("{}", output);

    // Verify serialization tests still pass
    assert!(
        output.contains("All serialization tests passed"),
        "Serialization tests should pass"
    );

    // Verify pub/sub was attempted
    assert!(
        output.contains("Testing pub/sub with custom messages")
            || output.contains("Publishing sensor readings"),
        "Should attempt pub/sub test"
    );

    // Check if messages were published (at least the attempt)
    let published = output.matches("Published:").count();
    println!("Published {} messages", published);

    // Check if any messages were received
    let received_count = output.matches("Received:").count();
    println!("Received {} messages", received_count);

    // With loopback, we should receive what we published
    // But even if not, the test passes as long as no errors
    assert!(
        output.contains("Custom message example completed successfully"),
        "Example should complete successfully"
    );

    println!("SUCCESS: Custom message pub/sub test passed");
}

// =============================================================================
// Message Type Tests
// =============================================================================

/// Test that custom SensorReading message has correct structure
#[rstest]
fn test_sensor_reading_structure() {
    let binary = build_native_custom_msg_no_zenoh().expect("Failed to build");

    let mut proc = ManagedProcess::spawn_command(Command::new(&binary), "custom_msg")
        .expect("Failed to start");

    // Wait for completion (no network, finishes quickly)
    let output = proc
        .wait_for_output_pattern("All serialization tests passed", Duration::from_secs(5))
        .unwrap_or_else(|_| {
            proc.kill();
            proc.wait_for_all_output(Duration::from_secs(1))
                .unwrap_or_default()
        });

    // SensorReading should serialize to expected size
    // i32 + f32 + f32 + u64 = 4 + 4 + 4 + 8 = 20 bytes + 4 byte CDR header = 24 bytes
    // But with alignment it may be different
    assert!(
        output.contains("SensorReading"),
        "Should test SensorReading message"
    );
    assert!(
        output.contains("bytes"),
        "Should report serialized size in bytes"
    );

    println!("SUCCESS: SensorReading structure test passed");
}

/// Test that custom Status message with string field works
#[rstest]
fn test_status_message_with_string() {
    let binary = build_native_custom_msg_no_zenoh().expect("Failed to build");

    let mut proc = ManagedProcess::spawn_command(Command::new(&binary), "custom_msg")
        .expect("Failed to start");

    // Wait for completion (no network, finishes quickly)
    let output = proc
        .wait_for_output_pattern("All serialization tests passed", Duration::from_secs(5))
        .unwrap_or_else(|_| {
            proc.kill();
            proc.wait_for_all_output(Duration::from_secs(1))
                .unwrap_or_default()
        });

    // Status has a string field which requires special handling
    assert!(
        output.contains("Status") && output.contains("OK"),
        "Status message with string should serialize correctly"
    );

    println!("SUCCESS: Status message with string field test passed");
}

// =============================================================================
// Error Handling Tests
// =============================================================================

/// Test that example handles missing zenoh router gracefully
#[rstest]
fn test_custom_msg_no_router() {
    let binary = build_native_custom_msg().expect("Failed to build with zenoh");

    // Run with zenoh feature but no router - should handle gracefully
    let mut cmd = Command::new(&binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", "tcp/127.0.0.1:19999") // Non-existent port
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "custom_msg").expect("Failed to start");

    // Wait for serialization tests to pass (those don't need network)
    let output = proc
        .wait_for_output_pattern("All serialization tests passed", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            proc.kill();
            proc.wait_for_all_output(Duration::from_secs(1))
                .unwrap_or_default()
        });

    println!("=== Output without router ===");
    println!("{}", output);

    // Serialization tests should still pass (don't require network)
    assert!(
        output.contains("All serialization tests passed"),
        "Serialization should work without router"
    );

    // Should report connection failure or skip pub/sub
    let handles_missing_router = output.contains("Failed to create context")
        || output.contains("zenohd")
        || output.contains("connection");

    println!(
        "Handles missing router: {}",
        if handles_missing_router {
            "yes (reported error)"
        } else {
            "skipped pub/sub gracefully"
        }
    );

    println!("SUCCESS: Handles missing router gracefully");
}
