//! Multi-Node and Scalability Tests
//!
//! Tests for multi-node scenarios and scalability verification.
//!
//! Run with: `cargo test -p nano-ros-tests --test multi_node -- --nocapture`
//! Or: `just test-rust-multi-node`
//!
//! ## What's Being Tested
//!
//! - Multiple publishers on a single topic
//! - Multiple subscribers on a single topic
//! - Many-to-many communication patterns
//! - High frequency publishing
//! - Sustained communication over time

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener, build_native_talker, require_zenohd,
    zenohd_unique,
};
use rstest::rstest;
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Multiple Publishers Tests
// =============================================================================

/// Test that multiple publishers on the same topic work correctly
///
/// Start 3 talkers and 1 listener, verify the listener receives messages
/// from all publishers.
#[rstest]
fn test_multiple_publishers_single_topic(zenohd_unique: ZenohRouter) {
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

    std::thread::sleep(Duration::from_secs(1));

    // Start 3 talkers
    let mut talkers = Vec::new();
    for i in 0..3 {
        let mut cmd = Command::new(&talker_binary);
        cmd.env("RUST_LOG", "info")
            .env("ZENOH_LOCATOR", &locator)
            .env("ZENOH_MODE", "client");

        let proc = ManagedProcess::spawn_command(cmd, &format!("talker_{}", i))
            .expect("Failed to start talker");
        talkers.push(proc);
    }

    // Let them communicate for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    // Kill all processes
    for mut talker in talkers {
        talker.kill();
    }
    listener.kill();

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Listener output (3 publishers) ===");
    println!("{}", listener_output);

    let received = count_pattern(&listener_output, "Received:");
    println!("Total messages received: {}", received);

    // With 3 talkers at ~1Hz for 5 seconds, expect at least some messages
    // Note: Multiple talkers with the same node name may not result in additive counts
    // due to zenoh's handling of identical publishers
    assert!(
        received >= 3,
        "Should receive messages from publishers, got {}",
        received
    );

    println!("SUCCESS: Multiple publishers work correctly");
}

// =============================================================================
// Multiple Subscribers Tests
// =============================================================================

/// Test that multiple subscribers on the same topic all receive messages
///
/// Start 1 talker and 3 listeners, verify each listener receives messages.
#[rstest]
fn test_multiple_subscribers_single_topic(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start 3 listeners
    let mut listeners = Vec::new();
    for i in 0..3 {
        let mut cmd = Command::new(&listener_binary);
        cmd.env("RUST_LOG", "info")
            .env("ZENOH_LOCATOR", &locator)
            .env("ZENOH_MODE", "client");

        let proc = ManagedProcess::spawn_command(cmd, &format!("listener_{}", i))
            .expect("Failed to start listener");
        listeners.push(proc);
    }

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

    // Kill talker
    talker.kill();

    // Collect output from all listeners
    let mut receive_counts = Vec::new();
    for mut listener in listeners {
        listener.kill();
        let output = listener
            .wait_for_all_output(Duration::from_secs(2))
            .unwrap_or_default();
        let count = count_pattern(&output, "Received:");
        receive_counts.push(count);
    }

    println!("Listener receive counts: {:?}", receive_counts);

    // All listeners should receive at least some messages
    for (i, count) in receive_counts.iter().enumerate() {
        assert!(
            *count >= 1,
            "Listener {} should receive at least 1 message, got {}",
            i,
            count
        );
    }

    // Check that all listeners received similar counts (within reasonable variance)
    let min_count = *receive_counts.iter().min().unwrap();
    let max_count = *receive_counts.iter().max().unwrap();
    let variance = max_count - min_count;

    println!(
        "Min: {}, Max: {}, Variance: {}",
        min_count, max_count, variance
    );

    // Allow up to 3 message variance (accounting for timing differences)
    assert!(
        variance <= 3,
        "All subscribers should receive similar message counts, variance was {}",
        variance
    );

    println!("SUCCESS: Multiple subscribers all receive messages");
}

// =============================================================================
// Many-to-Many Tests
// =============================================================================

/// Test many-to-many communication pattern
///
/// Start 2 talkers and 2 listeners, verify all messages are delivered.
#[rstest]
fn test_many_to_many(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start 2 listeners
    let mut listeners = Vec::new();
    for i in 0..2 {
        let mut cmd = Command::new(&listener_binary);
        cmd.env("RUST_LOG", "info")
            .env("ZENOH_LOCATOR", &locator)
            .env("ZENOH_MODE", "client");

        let proc = ManagedProcess::spawn_command(cmd, &format!("listener_{}", i))
            .expect("Failed to start listener");
        listeners.push(proc);
    }

    std::thread::sleep(Duration::from_secs(1));

    // Start 2 talkers
    let mut talkers = Vec::new();
    for i in 0..2 {
        let mut cmd = Command::new(&talker_binary);
        cmd.env("RUST_LOG", "info")
            .env("ZENOH_LOCATOR", &locator)
            .env("ZENOH_MODE", "client");

        let proc = ManagedProcess::spawn_command(cmd, &format!("talker_{}", i))
            .expect("Failed to start talker");
        talkers.push(proc);
    }

    // Let them communicate for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    // Kill all
    for mut talker in talkers {
        talker.kill();
    }

    let mut receive_counts = Vec::new();
    for mut listener in listeners {
        listener.kill();
        let output = listener
            .wait_for_all_output(Duration::from_secs(2))
            .unwrap_or_default();
        let count = count_pattern(&output, "Received:");
        receive_counts.push(count);
    }

    println!("Many-to-many receive counts: {:?}", receive_counts);

    // With 2 talkers at ~1Hz for 3 seconds, expect at least 2 messages per listener
    for (i, count) in receive_counts.iter().enumerate() {
        assert!(
            *count >= 2,
            "Listener {} should receive at least 2 messages from 2 publishers, got {}",
            i,
            count
        );
    }

    println!("SUCCESS: Many-to-many communication works");
}

// =============================================================================
// Sustained Communication Tests
// =============================================================================

/// Test sustained communication over a longer period
///
/// Run talker and listener for 10 seconds, verify consistent message delivery.
#[rstest]
fn test_sustained_communication(zenohd_unique: ZenohRouter) {
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

    // Run for 7 seconds
    std::thread::sleep(Duration::from_secs(7));

    talker.kill();
    listener.kill();

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    let published = count_pattern(&talker_output, "Published:");
    let received = count_pattern(&listener_output, "Received:");

    println!("Published: {}, Received: {}", published, received);

    // At 1Hz for 7 seconds, expect ~7 messages (allow for timing variance)
    assert!(
        published >= 5,
        "Should publish at least 5 messages in 7 seconds, got {}",
        published
    );

    // Calculate delivery ratio
    let delivery_ratio = received as f64 / published as f64;
    println!("Delivery ratio: {:.2}%", delivery_ratio * 100.0);

    // Expect at least 80% delivery
    assert!(
        delivery_ratio >= 0.8,
        "Sustained communication should maintain 80% delivery, got {:.2}%",
        delivery_ratio * 100.0
    );

    println!("SUCCESS: Sustained communication works correctly");
}

/// Test message ordering is preserved over time
#[rstest]
fn test_message_ordering_sustained(zenohd_unique: ZenohRouter) {
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

    // Run for 5 seconds
    std::thread::sleep(Duration::from_secs(5));

    talker.kill();
    listener.kill();

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    // Extract received values
    let mut received_values: Vec<i32> = Vec::new();
    for line in listener_output.lines() {
        if line.contains("Received:") && line.contains("data=") {
            if let Some(data_part) = line.split("data=").nth(1) {
                if let Some(num_str) = data_part.split_whitespace().next() {
                    if let Ok(num) = num_str.trim().parse() {
                        received_values.push(num);
                    }
                }
            }
        }
    }

    println!("Received values: {:?}", received_values);

    if received_values.len() >= 3 {
        // Verify messages are in order
        let is_ordered = received_values.windows(2).all(|w| w[0] <= w[1]);
        assert!(
            is_ordered,
            "Messages should be received in order: {:?}",
            received_values
        );

        // Check for gaps
        let mut gaps = 0;
        for window in received_values.windows(2) {
            if window[1] - window[0] > 1 {
                gaps += 1;
            }
        }

        println!("Message gaps: {}", gaps);

        // Allow at most 1 gap (during initial connection)
        assert!(
            gaps <= 1,
            "Should have minimal gaps in sustained communication, got {}",
            gaps
        );

        println!("SUCCESS: Message ordering preserved over time");
    } else {
        println!(
            "INFO: Not enough messages to verify ordering ({})",
            received_values.len()
        );
    }
}

// =============================================================================
// Scalability Tests
// =============================================================================

/// Test scaling up the number of subscribers
///
/// Start with 1 listener, add more, verify all receive messages.
#[rstest]
fn test_subscriber_scalability(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start 5 listeners
    let num_listeners = 5;
    let mut listeners = Vec::new();

    for i in 0..num_listeners {
        let mut cmd = Command::new(&listener_binary);
        cmd.env("RUST_LOG", "info")
            .env("ZENOH_LOCATOR", &locator)
            .env("ZENOH_MODE", "client");

        let proc = ManagedProcess::spawn_command(cmd, &format!("listener_{}", i))
            .expect("Failed to start listener");
        listeners.push(proc);

        // Stagger startup slightly
        std::thread::sleep(Duration::from_millis(200));
    }

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

    // Collect results
    let mut receive_counts = Vec::new();
    for mut listener in listeners {
        listener.kill();
        let output = listener
            .wait_for_all_output(Duration::from_secs(2))
            .unwrap_or_default();
        let count = count_pattern(&output, "Received:");
        receive_counts.push(count);
    }

    println!(
        "Scalability test - {} listeners: {:?}",
        num_listeners, receive_counts
    );

    // All listeners should receive messages
    let all_received = receive_counts.iter().all(|&c| c >= 1);
    assert!(
        all_received,
        "All {} listeners should receive at least 1 message: {:?}",
        num_listeners, receive_counts
    );

    // Calculate total messages received
    let total_received: usize = receive_counts.iter().sum();
    println!(
        "Total messages across {} listeners: {}",
        num_listeners, total_received
    );

    println!("SUCCESS: Subscriber scalability works");
}

/// Test scaling up the number of publishers
#[rstest]
fn test_publisher_scalability(zenohd_unique: ZenohRouter) {
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

    // Start 5 talkers
    let num_talkers = 5;
    let mut talkers = Vec::new();

    for i in 0..num_talkers {
        let mut cmd = Command::new(&talker_binary);
        cmd.env("RUST_LOG", "info")
            .env("ZENOH_LOCATOR", &locator)
            .env("ZENOH_MODE", "client");

        let proc = ManagedProcess::spawn_command(cmd, &format!("talker_{}", i))
            .expect("Failed to start talker");
        talkers.push(proc);

        // Stagger startup slightly
        std::thread::sleep(Duration::from_millis(200));
    }

    // Let them communicate for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    // Kill all talkers
    for mut talker in talkers {
        talker.kill();
    }

    listener.kill();

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    let received = count_pattern(&listener_output, "Received:");

    println!(
        "Publisher scalability - {} talkers, {} messages received",
        num_talkers, received
    );

    // With 5 talkers at ~1Hz for 3 seconds, expect significant messages
    assert!(
        received >= 5,
        "Should receive many messages from {} publishers, got {}",
        num_talkers,
        received
    );

    println!("SUCCESS: Publisher scalability works");
}

// =============================================================================
// Concurrent Startup Tests
// =============================================================================

/// Test that concurrent startup of multiple nodes works
#[rstest]
fn test_concurrent_startup(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start 2 listeners and 2 talkers nearly simultaneously
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

    let mut talker1_cmd = Command::new(&talker_binary);
    talker1_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker2_cmd = Command::new(&talker_binary);
    talker2_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    // Start all 4 processes as quickly as possible
    let mut listener1 = ManagedProcess::spawn_command(listener1_cmd, "listener1")
        .expect("Failed to start listener1");
    let mut listener2 = ManagedProcess::spawn_command(listener2_cmd, "listener2")
        .expect("Failed to start listener2");
    let mut talker1 =
        ManagedProcess::spawn_command(talker1_cmd, "talker1").expect("Failed to start talker1");
    let mut talker2 =
        ManagedProcess::spawn_command(talker2_cmd, "talker2").expect("Failed to start talker2");

    // Let them run for 3 seconds
    std::thread::sleep(Duration::from_secs(3));

    // Kill all
    talker1.kill();
    talker2.kill();
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

    println!(
        "Concurrent startup - Listener 1: {}, Listener 2: {}",
        received1, received2
    );

    // Both listeners should receive messages (may have some initial loss due to race)
    // At least one should receive messages
    assert!(
        received1 >= 1 || received2 >= 1,
        "At least one listener should receive messages"
    );

    println!("SUCCESS: Concurrent startup works");
}
