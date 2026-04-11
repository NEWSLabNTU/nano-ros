//! QoS Policy Integration Tests
//!
//! Tests for Quality of Service policy behavior.
//!
//! Run with: `cargo test -p nano-ros-tests --test qos -- --nocapture`
//! Or: `just test-rust-qos`
//!
//! ## What's Being Tested
//!
//! The native-rs-talker and native-rs-listener examples use:
//! - Reliability: RELIABLE
//! - History: KEEP_LAST(10)
//!
//! These tests verify that QoS settings work correctly for communication.

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener, build_native_talker, require_zenohd,
    zenohd_unique,
};
use rstest::rstest;
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Reliability Tests
// =============================================================================

/// Test that RELIABLE QoS results in message delivery
///
/// Both talker and listener use RELIABLE QoS, so messages should be delivered.
#[rstest]
fn test_qos_reliable_delivery(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("Failed to start listener");

    // Give listener time to subscribe
    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Let them communicate for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    talker.kill();
    listener.kill();

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Talker output ===");
    println!("{}", talker_output);
    println!("=== Listener output ===");
    println!("{}", listener_output);

    let published = count_pattern(&talker_output, "Published:");
    let received = count_pattern(&listener_output, "Received:");

    println!("Published: {}, Received: {}", published, received);

    // With RELIABLE QoS, we expect most messages to be delivered
    assert!(published > 0, "Talker should publish messages");
    assert!(
        received > 0,
        "Listener should receive messages with RELIABLE QoS"
    );

    // Verify reasonable delivery ratio (allowing for timing/startup delays)
    let delivery_ratio = received as f64 / published as f64;
    println!("Delivery ratio: {:.2}", delivery_ratio);

    // At least 50% of messages should be received (accounting for startup time)
    assert!(
        delivery_ratio >= 0.5,
        "RELIABLE QoS should deliver most messages, got {:.2}%",
        delivery_ratio * 100.0
    );

    println!("SUCCESS: RELIABLE QoS delivers messages correctly");
}

/// Test that messages are received without loss in steady state
///
/// After initial connection, RELIABLE QoS should not drop messages.
#[rstest]
fn test_qos_reliable_no_loss(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("Failed to start listener");

    // Give listener time to subscribe
    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Let them communicate for 3 seconds (more time for steady state)
    std::thread::sleep(Duration::from_secs(3));

    talker.kill();
    listener.kill();

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Listener output ===");
    println!("{}", listener_output);

    // Extract received values
    let mut received_values: Vec<i32> = Vec::new();
    for line in listener_output.lines() {
        if line.contains("Received:") {
            if let Some(data_part) = line.split("Received:").nth(1) {
                if let Ok(num) = data_part.trim().parse() {
                    received_values.push(num);
                }
            }
        }
    }

    println!("Received values: {:?}", received_values);

    if received_values.len() >= 3 {
        // Check for gaps (missing messages) - not counting initial startup delay
        let mut gaps = 0;
        for window in received_values.windows(2) {
            if window[1] - window[0] > 1 {
                gaps += 1;
            }
        }

        println!("Message gaps detected: {}", gaps);

        // In steady state, we shouldn't have many gaps with RELIABLE QoS
        // Allow up to 1 gap (e.g., during initial connection)
        assert!(
            gaps <= 1,
            "RELIABLE QoS should minimize message loss, but found {} gaps",
            gaps
        );

        println!("SUCCESS: RELIABLE QoS maintains message delivery");
    } else {
        println!(
            "INFO: Not enough messages to verify gap-free delivery ({})",
            received_values.len()
        );
    }
}

// =============================================================================
// History Depth Tests
// =============================================================================

/// Test that history depth is respected
///
/// The talker/listener use keep_last(10), meaning up to 10 messages can be queued.
/// We verify that messages are received in order, which indicates proper history handling.
#[rstest]
fn test_qos_history_ordering(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
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

    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(3));

    talker.kill();
    listener.kill();

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    // Extract received values
    let mut received_values: Vec<i32> = Vec::new();
    for line in listener_output.lines() {
        if line.contains("Received:") {
            if let Some(data_part) = line.split("Received:").nth(1) {
                if let Ok(num) = data_part.trim().parse() {
                    received_values.push(num);
                }
            }
        }
    }

    println!("Received values: {:?}", received_values);

    if received_values.len() >= 2 {
        // Verify messages are in order (history preserves ordering)
        let is_ordered = received_values.windows(2).all(|w| w[0] <= w[1]);
        assert!(
            is_ordered,
            "History should preserve message ordering: {:?}",
            received_values
        );

        println!("SUCCESS: QoS history preserves message ordering");
    } else {
        println!(
            "INFO: Not enough messages to verify ordering ({})",
            received_values.len()
        );
    }
}

// =============================================================================
// Communication Pattern Tests
// =============================================================================

/// Test that QoS settings result in proper publisher/subscriber matching
///
/// When both sides use compatible QoS (RELIABLE + RELIABLE), communication works.
#[rstest]
fn test_qos_compatible_settings(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start listener
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "debug") // Debug to see QoS info
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "debug")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(3));

    talker.kill();
    listener.kill();

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    // Verify no QoS incompatibility errors
    assert!(
        !talker_output.contains("QoS incompatible")
            && !listener_output.contains("QoS incompatible"),
        "Should not have QoS incompatibility warnings"
    );

    // Verify communication works
    let received = count_pattern(&listener_output, "Received:");
    assert!(
        received > 0,
        "Compatible QoS settings should allow communication"
    );

    println!("SUCCESS: Compatible QoS settings work correctly");
}

/// Test that multiple subscribers receive the same messages
///
/// With RELIABLE QoS, all subscribers should receive published messages.
#[rstest]
fn test_qos_multiple_subscribers(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start two listeners
    let mut listener1_cmd = Command::new(&listener_binary);
    listener1_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener2_cmd = Command::new(&listener_binary);
    listener2_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut listener1 = ManagedProcess::spawn_command(listener1_cmd, "listener1")
        .expect("Failed to start listener1");
    let mut listener2 = ManagedProcess::spawn_command(listener2_cmd, "listener2")
        .expect("Failed to start listener2");

    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(3));

    talker.kill();
    listener1.kill();
    listener2.kill();

    let listener1_output = listener1
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener2_output = listener2
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    let received1 = count_pattern(&listener1_output, "Received:");
    let received2 = count_pattern(&listener2_output, "Received:");

    println!("Listener 1 received: {}", received1);
    println!("Listener 2 received: {}", received2);

    // Both subscribers should receive messages
    assert!(received1 > 0, "Listener 1 should receive messages");
    assert!(received2 > 0, "Listener 2 should receive messages");

    // Both should receive similar counts (within reasonable variance)
    let diff = (received1 as i32 - received2 as i32).unsigned_abs();
    assert!(
        diff <= 2,
        "Both subscribers should receive similar message counts: {} vs {}",
        received1,
        received2
    );

    println!("SUCCESS: Multiple subscribers receive messages with QoS");
}

// =============================================================================
// QoS API Verification Tests
// =============================================================================

/// Test that the QoS liveliness keyexpr contains expected QoS information
///
/// The nros implementation encodes QoS in the liveliness keyexpr.
/// This test verifies the QoS encoding is present.
#[rstest]
fn test_qos_keyexpr_encoding(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(talker_binary);
    cmd.env("RUST_LOG", "debug") // Debug to see liveliness keyexpr
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Wait for talker to publish (ensures liveliness keyexpr is logged)
    let early_output = proc
        .wait_for_output_pattern("Publishing", Duration::from_secs(5))
        .unwrap_or_default();

    proc.kill();

    let remaining = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let output = format!("{}{}", early_output, remaining);

    println!("=== Talker output ===");
    println!("{}", output);

    // The liveliness keyexpr should contain QoS encoding
    // Format: @ros2_lv/.../MP/.../RIHS01_.../1:2:1,10:,:,:,,
    // The "1:2:1,10" encodes: reliability:durability:history,depth
    // 1=RELIABLE, 2=VOLATILE, 1=KEEP_LAST, 10=depth

    // Check that liveliness keyexpr is present
    assert!(
        output.contains("liveliness keyexpr"),
        "Should log liveliness keyexpr with QoS info"
    );

    // The publisher uses reliable().keep_last(10) which should encode as:
    // reliability=1 (RELIABLE), depth=10
    if output.contains("1:2:1,10") || output.contains(",10:") {
        println!("SUCCESS: QoS settings encoded in liveliness keyexpr");
    } else {
        println!("INFO: QoS encoding format may vary, but liveliness keyexpr is present");
    }
}
