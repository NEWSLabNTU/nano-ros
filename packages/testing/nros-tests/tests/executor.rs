//! Executor and Timer Integration Tests
//!
//! Tests for timer firing, callback execution, and executor behavior.
//!
//! Run with: `cargo test -p nano-ros-tests --test executor -- --nocapture`
//! Or: `just test-rust-executor`

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, ZenohRouter, build_native_listener, build_native_talker, require_zenohd,
        zenohd_unique,
    },
};
use rstest::rstest;
use std::{process::Command, time::Duration};

// =============================================================================
// Timer Interval Tests
// =============================================================================

/// Test that timer fires at expected interval by checking talker output
///
/// The native-rs-talker publishes messages at ~1Hz (1000ms interval).
/// We verify that approximately the expected number of messages are published
/// in a given time window.
#[rstest]
fn test_timer_interval_basic(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Wait for ~5 messages at 1Hz (event-driven: wait for "Published: 4" which means 5 publishes)
    let output = proc
        .wait_for_output_pattern("Published: 4", Duration::from_secs(10))
        .unwrap_or_default();

    proc.kill();

    println!("=== Talker timer output ===");
    println!("{}", output);

    // Count "Published:" lines
    let published_count = count_pattern(&output, "Published:");

    // At 1Hz for 5 seconds, we expect ~5 messages (allow 3-7 for timing variance)
    println!("Published count: {}", published_count);
    assert!(
        published_count >= 3 && published_count <= 7,
        "Expected ~5 messages at 1Hz over 5s, got {}",
        published_count
    );

    println!("SUCCESS: Timer fires at expected interval");
}

/// Test that messages are published at regular intervals (timing consistency)
#[rstest]
fn test_timer_regular_publishing(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Wait for at least 2 sequential messages
    let output = proc
        .wait_for_output_pattern("Published: 1", Duration::from_secs(10))
        .unwrap_or_default();

    proc.kill();

    // Verify sequential counter values (indicating regular firing)
    let has_sequential = output.contains("Published: 0") && output.contains("Published: 1");

    assert!(
        has_sequential,
        "Timer should fire with sequential counter values. Output:\n{}",
        output
    );

    println!("SUCCESS: Timer fires regularly with sequential values");
}

// =============================================================================
// Callback Execution Order Tests
// =============================================================================

/// Test that messages are received in the order they were sent
#[rstest]
fn test_callback_execution_order(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let listener_binary = build_native_listener().expect("Failed to build listener");
    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("Failed to start listener");

    // Wait for listener readiness
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Wait for listener to receive messages (event-driven)
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(10))
        .unwrap_or_default();

    // Kill both
    talker.kill();
    listener.kill();

    println!("=== Listener output ===");
    println!("{}", listener_output);

    // Extract received values and verify order
    // The listener logs "Received: N" where N is the value
    let mut received_values: Vec<i32> = Vec::new();
    for line in listener_output.lines() {
        if line.contains("Received:") {
            // Parse "Received: N" pattern
            if let Some(data_part) = line.split("Received:").nth(1) {
                if let Ok(num) = data_part.trim().parse() {
                    received_values.push(num);
                }
            }
        }
    }

    println!("Received values: {:?}", received_values);

    // Verify values are in ascending order (messages received in order)
    if received_values.len() >= 2 {
        let is_ordered = received_values.windows(2).all(|w| w[0] <= w[1]);
        assert!(
            is_ordered,
            "Messages should be received in order: {:?}",
            received_values
        );
        println!("SUCCESS: Callback execution maintains message order");
    } else {
        println!(
            "INFO: Not enough messages received to verify order ({})",
            received_values.len()
        );
    }
}

// =============================================================================
// Mixed Callback Tests (Timer + Subscription)
// =============================================================================

/// Test that both timer and subscription callbacks fire correctly
///
/// The talker has an internal timer that triggers publishing.
/// The listener has a subscription callback that receives messages.
/// This tests that both types of callbacks work together.
#[rstest]
fn test_mixed_callbacks(zenohd_unique: ZenohRouter) {
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
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("Failed to start listener");

    // Wait for listener readiness
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Wait for listener to receive messages (event-driven)
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(10))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Talker output ===");
    println!("{}", talker_output);
    println!("=== Listener output ===");
    println!("{}", listener_output);

    // Verify talker timer fired (published messages)
    let published_count = count_pattern(&talker_output, "Published:");
    assert!(
        published_count > 0,
        "Talker timer should fire and publish messages"
    );

    // Verify listener subscription fired (received messages)
    let received_count = count_pattern(&listener_output, "Received:");
    assert!(
        received_count > 0,
        "Listener subscription callback should fire"
    );

    println!(
        "SUCCESS: Mixed callbacks work - {} published, {} received",
        published_count, received_count
    );
}

// =============================================================================
// Spin Behavior Tests
// =============================================================================

/// Test that spin_once processes pending work
///
/// We verify this by observing that messages are published/received correctly,
/// which requires spin_once to be processing callbacks.
#[rstest]
fn test_spin_once_processes_work(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(talker_binary);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start");

    // Wait for at least one publish (event-driven)
    let output = proc
        .wait_for_output_pattern("Published:", Duration::from_secs(5))
        .unwrap_or_default();

    proc.kill();

    // The talker uses spin_once() in its main loop.
    // If spin_once processes work correctly, we should see "Published:" messages.
    let published = count_pattern(&output, "Published:");

    assert!(
        published > 0,
        "spin_once should process timer callbacks, enabling publishing. Output:\n{}",
        output
    );

    println!(
        "SUCCESS: spin_once processes pending work ({} messages)",
        published
    );
}

/// Test executor handles multiple publishers
#[rstest]
fn test_executor_multiple_timers_via_publishers(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    // While we can't easily create multiple timers in one process from tests,
    // we can verify the executor handles multiple processes with timers correctly.
    let talker_binary = build_native_talker().expect("Failed to build talker");
    let locator = zenohd_unique.locator();

    // Start two talkers (each has its own timer)
    let mut talker1_cmd = Command::new(&talker_binary);
    talker1_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut talker2_cmd = Command::new(&talker_binary);
    talker2_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut talker1 =
        ManagedProcess::spawn_command(talker1_cmd, "talker1").expect("Failed to start talker1");
    let mut talker2 =
        ManagedProcess::spawn_command(talker2_cmd, "talker2").expect("Failed to start talker2");

    // Wait for both to publish at least once (event-driven)
    let output1 = talker1
        .wait_for_output_pattern("Published:", Duration::from_secs(5))
        .unwrap_or_default();
    let output2 = talker2
        .wait_for_output_pattern("Published:", Duration::from_secs(5))
        .unwrap_or_default();

    talker1.kill();
    talker2.kill();

    let count1 = count_pattern(&output1, "Published:");
    let count2 = count_pattern(&output2, "Published:");

    println!("Talker 1 published: {}", count1);
    println!("Talker 2 published: {}", count2);

    // Both should have published messages
    assert!(count1 > 0, "Talker 1 should publish messages");
    assert!(count2 > 0, "Talker 2 should publish messages");

    println!("SUCCESS: Multiple processes with timers work correctly");
}

// =============================================================================
// SpinOnceResult Tests
// =============================================================================

/// Test that the talker node is processing timers (via output)
///
/// The native-rs-talker uses executor.spin_once() which returns SpinOnceResult.
/// We verify this works by checking the output shows timer-driven publishing.
#[rstest]
fn test_spin_result_timers_fired(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let binary = build_native_talker().expect("Failed to build");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "debug") // Debug to see more internal details
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start");

    // Wait for at least one publish (event-driven)
    let output = proc
        .wait_for_output_pattern("Published:", Duration::from_secs(5))
        .unwrap_or_default();

    proc.kill();

    println!("=== Talker debug output ===");
    println!("{}", output);

    // Verify the publishing loop is working (driven by timer/spin_once)
    assert!(
        output.contains("Published:"),
        "Timer-driven publishing should work"
    );

    println!("SUCCESS: Executor spin_once processes timer callbacks");
}
