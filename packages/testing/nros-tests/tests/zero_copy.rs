//! Zero-Copy Receive Integration Tests
//!
//! Tests the zero-copy code path enabled by the `unstable-zenoh-api` feature.
//! The zero-copy path is transparent — `create_subscription_with_info()` automatically
//! uses it when the feature is enabled. These tests verify:
//! - Binary builds and starts correctly with the zero-copy path
//! - End-to-end message flow through the zero-copy callback trampoline
//! - MessageInfo (sequence number, GID) is correctly passed through

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener_zero_copy, build_native_talker,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::time::Duration;

// =============================================================================
// Zero-Copy Listener Startup
// =============================================================================

/// Test that the listener binary compiles and starts with the zero-copy path.
#[rstest]
fn test_zero_copy_listener_starts(zenohd_unique: ZenohRouter) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let listener_path =
        build_native_listener_zero_copy().expect("Failed to build zero-copy listener");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(listener_path);
    cmd.env("NROS_LOCATOR", &locator).env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(cmd, "zero-copy-listener")
        .expect("Failed to start zero-copy listener");

    match listener.wait_for_output_pattern("Waiting for", Duration::from_secs(10)) {
        Ok(_) => eprintln!("zero-copy listener started successfully"),
        Err(_) => {
            if listener.is_running() {
                eprintln!("zero-copy listener running (no readiness marker yet)");
            } else {
                panic!("zero-copy listener exited early");
            }
        }
    }
}

// =============================================================================
// Zero-Copy Talker ↔ Listener Communication
// =============================================================================

/// Test end-to-end message flow through the zero-copy callback trampoline.
///
/// Uses the standard talker (no zero-copy needed for publishing) and the
/// zero-copy listener. Verifies that messages flow correctly.
#[rstest]
fn test_zero_copy_talker_listener(zenohd_unique: ZenohRouter) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_path = build_native_talker().expect("Failed to build talker");
    let listener_path =
        build_native_listener_zero_copy().expect("Failed to build zero-copy listener");
    let locator = zenohd_unique.locator();

    // Start zero-copy listener first (subscriber before publisher)
    let mut listener_cmd = Command::new(listener_path);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "zero-copy-listener")
        .expect("Failed to start zero-copy listener");

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .expect("Zero-copy listener did not start");

    // Stabilization delay: let subscription propagate through zenohd
    std::thread::sleep(Duration::from_secs(3));

    // Start standard talker
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let _talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Wait for listener to receive multiple messages
    let output = listener
        .wait_for_all_output(Duration::from_secs(30))
        .expect("Failed to collect listener output");

    let result = nros_tests::output::assert_listener(&output, 3);
    let received_count = result.received_count;

    eprintln!(
        "zero-copy talker→listener: {} messages received",
        received_count
    );
}

// =============================================================================
// Zero-Copy MessageInfo Verification
// =============================================================================

/// Test that MessageInfo (sequence number, GID) is correctly passed through
/// the zero-copy trampoline.
///
/// The listener prints trace-level lines like:
///   seq=N gid=XXXX ts=T
/// We parse these to verify monotonic sequence numbers and consistent GID.
#[rstest]
fn test_zero_copy_message_info(zenohd_unique: ZenohRouter) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_path = build_native_talker().expect("Failed to build talker");
    let listener_path =
        build_native_listener_zero_copy().expect("Failed to build zero-copy listener");
    let locator = zenohd_unique.locator();

    // Start zero-copy listener with RUST_LOG=trace to get MessageInfo output
    let mut listener_cmd = Command::new(listener_path);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "trace");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "zero-copy-listener")
        .expect("Failed to start zero-copy listener");

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .expect("Zero-copy listener did not start");

    // Stabilization delay
    std::thread::sleep(Duration::from_secs(3));

    // Start talker
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd.env("NROS_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Wait for several messages
    std::thread::sleep(Duration::from_secs(6));

    // Kill processes and collect output
    talker.kill();
    listener.kill();
    let output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("Zero-copy listener trace output:\n{}", output);

    // Parse seq= values from trace output
    let seq_values: Vec<i64> = output
        .lines()
        .filter_map(|line| {
            if let Some(pos) = line.find("seq=") {
                let rest = &line[pos + 4..];
                let end = rest.find(' ').unwrap_or(rest.len());
                rest[..end].parse::<i64>().ok()
            } else {
                None
            }
        })
        .collect();

    eprintln!("Parsed sequence numbers: {:?}", seq_values);

    assert!(
        seq_values.len() >= 2,
        "Need at least 2 sequence numbers to verify increment, got {}",
        seq_values.len()
    );

    // Verify monotonic increment
    for window in seq_values.windows(2) {
        assert!(
            window[1] > window[0],
            "Sequence numbers should increment: {} should be > {}",
            window[1],
            window[0]
        );
    }

    // Parse gid= values from trace output
    let gid_values: Vec<String> = output
        .lines()
        .filter_map(|line| {
            if let Some(pos) = line.find("gid=") {
                let rest = &line[pos + 4..];
                let end = rest.find(' ').unwrap_or(rest.len());
                Some(rest[..end].to_string())
            } else {
                None
            }
        })
        .collect();

    eprintln!("Parsed GIDs: {:?}", gid_values);

    assert!(
        gid_values.len() >= 2,
        "Need at least 2 GID values to verify consistency, got {}",
        gid_values.len()
    );

    // Verify all GIDs are identical (same publisher)
    let first_gid = &gid_values[0];
    for (i, gid) in gid_values.iter().enumerate() {
        assert_eq!(
            gid, first_gid,
            "GID at message {} ({}) should match first GID ({})",
            i, gid, first_gid
        );
    }

    // Verify GID is not all zeros
    assert_ne!(
        first_gid, "00000000",
        "GID should not be all zeros (should contain real publisher ID)"
    );

    eprintln!(
        "[PASS] Zero-copy MessageInfo: {} messages, seq monotonic, GID consistent ({})",
        seq_values.len(),
        first_gid
    );
}
