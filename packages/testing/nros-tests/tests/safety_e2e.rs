//! E2E Safety Protocol Integration Tests
//!
//! Tests the full safety-e2e stack:
//! - Publisher CRC computation → transport → subscriber CRC validation
//! - Sequence tracking across multiple messages
//! - Backward compatibility (safety publisher → standard listener)

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_listener, build_native_listener_safety,
    build_native_talker_safety, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::time::Duration;

// =============================================================================
// Full-Stack E2E: safety talker + safety listener
// =============================================================================

/// Test that safety-e2e talker + listener communicate correctly.
///
/// The listener should print `[SAFETY]` status lines showing valid CRC and
/// zero sequence gap for sequential messages.
#[rstest]
fn test_safety_e2e_talker_listener(zenohd_unique: ZenohRouter) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let talker_path = build_native_talker_safety().expect("Failed to build safety talker");
    let listener_path = build_native_listener_safety().expect("Failed to build safety listener");
    let locator = zenohd_unique.locator();

    // Start listener first (subscriber before publisher)
    let mut listener_cmd = Command::new(listener_path);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "safety-listener")
        .expect("Failed to start safety listener");

    // Wait for listener readiness
    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .expect("Listener did not start");

    // Stabilization delay: let subscription propagate through zenohd before
    // the talker starts publishing. Without this, early messages are lost
    // under parallel test load (zenoh subscription discovery is async).
    std::thread::sleep(Duration::from_secs(3));

    // Start talker
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let _talker = ManagedProcess::spawn_command(talker_cmd, "safety-talker")
        .expect("Failed to start safety talker");

    // Wait for listener to receive multiple safety-validated messages.
    // The listener prints lines like:
    //   [N] Received: data=M [SAFETY] seq_gap=0 dup=false crc=ok
    // Allow 30s: under heavy parallel load, zenoh session establishment
    // can take several seconds (talker publishes every 1s).
    let output = listener
        .wait_for_all_output(Duration::from_secs(10))
        .expect("Failed to collect listener output");

    let safety_ok_count = output.matches("crc=ok").count();
    let safety_fail_count = output.matches("crc=FAIL").count();

    eprintln!(
        "safety-e2e results: {} ok, {} fail",
        safety_ok_count, safety_fail_count
    );

    assert!(
        safety_ok_count >= 3,
        "Expected at least 3 valid safety messages, got {}. Output:\n{}",
        safety_ok_count,
        output
    );
    assert_eq!(
        safety_fail_count, 0,
        "Expected no CRC failures. Output:\n{}",
        output
    );

    // Verify sequential messages have gap=0
    let gap_zero_count = output.matches("seq_gap=0").count();
    assert!(
        gap_zero_count >= 3,
        "Expected sequential messages with gap=0, got {} matches. Output:\n{}",
        gap_zero_count,
        output
    );
}

// =============================================================================
// Mixed Mode: safety talker + standard listener
// =============================================================================

/// Test backward compatibility: safety-e2e talker → standard listener.
///
/// The standard listener (without safety-e2e) should still receive messages
/// normally. The extra 4 CRC bytes in the attachment are ignored by the
/// standard subscriber path.
#[rstest]
fn test_safety_talker_standard_listener(zenohd_unique: ZenohRouter) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let talker_path = build_native_talker_safety().expect("Failed to build safety talker");
    let listener_path = build_native_listener().expect("Failed to build standard listener");
    let locator = zenohd_unique.locator();

    // Start standard listener first
    let mut listener_cmd = Command::new(listener_path);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "std-listener")
        .expect("Failed to start standard listener");

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .expect("Standard listener did not start");

    // Stabilization delay: let subscription propagate through zenohd
    std::thread::sleep(Duration::from_secs(3));

    // Start safety-e2e talker
    let mut talker_cmd = Command::new(talker_path);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let _talker = ManagedProcess::spawn_command(talker_cmd, "safety-talker")
        .expect("Failed to start safety talker");

    // Standard listener should receive messages (prints "Received: data=")
    // Allow 30s for session establishment under parallel test load.
    let output = listener
        .wait_for_all_output(Duration::from_secs(10))
        .expect("Failed to collect listener output");

    let received_count = output.matches("Received:").count();

    eprintln!(
        "mixed-mode: standard listener received {} messages",
        received_count
    );
    assert!(
        received_count >= 2,
        "Expected at least 2 messages, got {}. Output:\n{}",
        received_count,
        output
    );
}
