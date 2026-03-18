//! Large message and throughput stress tests
//!
//! Tests large message publishing, E2E data integrity, overflow detection,
//! and throughput at various rates for both zenoh and XRCE backends.
//!
//! Prerequisites:
//!   Zenoh tests: zenohd (built automatically)
//!   XRCE tests: just build-xrce-agent

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, XrceAgent, ZenohRouter, qemu_large_msg_test_binary, require_xrce_agent,
    require_zenohd, xrce_stress_test_binary, zenoh_stress_test_binary,
    zenoh_stress_test_large_buf_binary, zenohd_unique,
};
use rstest::rstest;
use std::path::PathBuf;
use std::time::Duration;

// =============================================================================
// Zenoh Large Message Tests
// =============================================================================

/// Test that publish_raw succeeds for various payload sizes (publish-only, no listener).
#[rstest]
fn test_zenoh_large_publish_sizes(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Test sizes from small to large (up to 32KB)
    for &size in &[64, 256, 1024, 4096, 8192, 32768] {
        let mut cmd = Command::new(&zenoh_stress_test_binary);
        cmd.env("ZENOH_LOCATOR", &locator)
            .env("MODE", "talker")
            .env("PAYLOAD_SIZE", size.to_string())
            .env("PUBLISH_COUNT", "3")
            .env("PUBLISH_INTERVAL_MS", "10");
        let mut proc = ManagedProcess::spawn_command(cmd, &format!("zenoh-stress-{}", size))
            .expect("Failed to start stress test");

        let output = proc
            .wait_for_output_pattern("PUBLISH_DONE:", Duration::from_secs(15))
            .unwrap_or_default();

        proc.kill();

        let published = count_pattern(&output, "Published:");
        assert!(
            published >= 2,
            "Expected at least 2 publishes at size={}, got {}.\nOutput:\n{}",
            size,
            published,
            output,
        );
    }
}

/// Test E2E data integrity: talker sends 512B payloads, listener validates them.
#[rstest]
fn test_zenoh_e2e_integrity(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "512")
        .env("EXPECTED_COUNT", "20")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "zenoh-stress-listener")
        .expect("Failed to start listener");

    // Wait for listener readiness
    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(5));

    // Stabilization delay for zenoh subscription propagation
    std::thread::sleep(Duration::from_secs(2));

    // Start talker
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "512")
        .env("PUBLISH_COUNT", "20")
        .env("PUBLISH_INTERVAL_MS", "50");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "zenoh-stress-talker")
        .expect("Failed to start talker");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(20))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);

    // Parse RECV_DONE line
    let received = count_pattern(&listener_output, "Received:");
    let invalid = count_pattern(&listener_output, "valid=false");

    assert!(
        received >= 10,
        "Expected at least 10 received messages, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
    assert_eq!(
        invalid, 0,
        "Expected 0 invalid messages, got {}.\nOutput:\n{}",
        invalid, listener_output,
    );
}

/// Test that oversized payloads (larger than receiver buffer) are detected.
#[rstest]
fn test_zenoh_overflow_detection(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener expecting 512B payloads
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "512")
        .env("EXPECTED_COUNT", "5")
        .env("TIMEOUT_SECS", "15");
    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "zenoh-stress-overflow-listener")
            .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(5));
    std::thread::sleep(Duration::from_secs(2));

    // Talker sends 2048B payloads (larger than default 1024B receiver buffer)
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "2048")
        .env("PUBLISH_COUNT", "5")
        .env("PUBLISH_INTERVAL_MS", "100");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "zenoh-stress-overflow-talker")
        .expect("Failed to start talker");

    // Wait for listener to time out or detect overflow
    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Overflow test output:\n{}", listener_output);

    // The listener either receives 0 messages (overflow drops them) or
    // receives them with validation failure (size mismatch).
    // Both are acceptable overflow detection behaviors.
    let valid_count = count_pattern(&listener_output, "valid=true");
    assert_eq!(
        valid_count, 0,
        "Expected 0 valid messages (overflow should cause size mismatch or drop), got {}.\nOutput:\n{}",
        valid_count, listener_output,
    );

    // Verify that the MessageTooLarge error was actually reported
    let overflow_errors = count_pattern(&listener_output, "Receive error");
    assert!(
        overflow_errors >= 1,
        "Expected at least 1 overflow error (MessageTooLarge), got 0.\nOutput:\n{}",
        listener_output,
    );
}

/// E2E test: talker sends 4096B payloads, listener with 8192B shim buffer receives them.
///
/// The listener is built with `ZPICO_SUBSCRIBER_BUFFER_SIZE=8192` so it can fit
/// 4096B payloads (which would overflow the default 1024B buffer). The talker
/// uses the same large-buf binary — publish path has no shim buffer constraint,
/// but using the same binary simplifies the test.
#[rstest]
fn test_zenoh_e2e_large_receive(
    zenohd_unique: ZenohRouter,
    zenoh_stress_test_large_buf_binary: PathBuf,
) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Listener (built with 8192B shim buffer)
    let mut listener_cmd = Command::new(&zenoh_stress_test_large_buf_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "4096")
        .env("EXPECTED_COUNT", "10")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "zenoh-large-recv-listener")
        .expect("spawn listener");

    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(5));
    std::thread::sleep(Duration::from_secs(2));

    // Talker (same large-buf binary)
    let mut talker_cmd = Command::new(&zenoh_stress_test_large_buf_binary);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "4096")
        .env("PUBLISH_COUNT", "10")
        .env("PUBLISH_INTERVAL_MS", "50");
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "zenoh-large-recv-talker").expect("spawn talker");

    let output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(20))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Large receive test output:\n{}", output);

    let received = count_pattern(&output, "Received:");
    let invalid = count_pattern(&output, "valid=false");

    assert!(
        received >= 5,
        "Expected >=5 received at 4096B, got {}.\nOutput:\n{}",
        received,
        output,
    );
    assert_eq!(
        invalid, 0,
        "Expected 0 invalid, got {}.\nOutput:\n{}",
        invalid, output,
    );
}

/// Throughput test at 100 Hz (10ms interval).
#[rstest]
fn test_zenoh_throughput_100hz(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "15");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "zenoh-100hz-listener")
        .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(5));
    std::thread::sleep(Duration::from_secs(2));

    // Start talker at 100 Hz
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "10");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "zenoh-100hz-talker")
        .expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    let received = count_pattern(&listener_output, "Received:");
    eprintln!("100Hz test: received {} messages", received);

    assert!(
        received >= 20,
        "Expected at least 20 messages at 100Hz, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
}

/// Throughput burst test (0ms interval).
#[rstest]
fn test_zenoh_throughput_burst(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "15");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "zenoh-burst-listener")
        .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(5));
    std::thread::sleep(Duration::from_secs(2));

    // Start talker with no delay (burst)
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "0");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "zenoh-burst-talker")
        .expect("Failed to start talker");

    // Wait longer for burst messages to propagate
    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    let received = count_pattern(&listener_output, "Received:");
    eprintln!("Burst test: received {} messages", received);

    assert!(
        received >= 1,
        "Expected at least 1 message in burst mode, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
}

// =============================================================================
// XRCE Large Message Tests
// =============================================================================

/// Test E2E data integrity over XRCE: 512B payloads.
#[rstest]
fn test_xrce_e2e_integrity(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start listener
    let mut listener_cmd = Command::new(&xrce_stress_test_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "512")
        .env("EXPECTED_COUNT", "20")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-stress-listener")
        .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    let mut talker_cmd = Command::new(&xrce_stress_test_binary);
    talker_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "512")
        .env("PUBLISH_COUNT", "20")
        .env("PUBLISH_INTERVAL_MS", "100");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "xrce-stress-talker")
        .expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(25))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("XRCE listener output:\n{}", listener_output);

    let received = count_pattern(&listener_output, "Received:");
    let invalid = count_pattern(&listener_output, "valid=false");

    assert!(
        received >= 5,
        "Expected at least 5 received messages, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
    assert_eq!(
        invalid, 0,
        "Expected 0 invalid messages, got {}.\nOutput:\n{}",
        invalid, listener_output,
    );

    drop(agent);
}

/// Test XRCE publish_raw succeeds for various sizes (publish-only).
#[rstest]
fn test_xrce_large_publish_sizes(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // XRCE supports larger messages via fragmented streams (Phase 40.3)
    for &size in &[64, 256, 1024, 4096, 12288] {
        let mut cmd = Command::new(&xrce_stress_test_binary);
        cmd.env("XRCE_AGENT_ADDR", &addr)
            .env("MODE", "talker")
            .env("PAYLOAD_SIZE", size.to_string())
            .env("PUBLISH_COUNT", "3")
            .env("PUBLISH_INTERVAL_MS", "50");
        let mut proc = ManagedProcess::spawn_command(cmd, &format!("xrce-stress-{}", size))
            .expect("Failed to start stress test");

        let output = proc
            .wait_for_output_pattern("PUBLISH_DONE:", Duration::from_secs(15))
            .unwrap_or_default();

        proc.kill();

        let published = count_pattern(&output, "Published:");
        assert!(
            published >= 2,
            "Expected at least 2 publishes at size={}, got {}.\nOutput:\n{}",
            size,
            published,
            output,
        );
    }

    drop(agent);
}

/// XRCE throughput test at 100 Hz.
#[rstest]
fn test_xrce_throughput_100hz(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut listener_cmd = Command::new(&xrce_stress_test_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-100hz-listener")
        .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(1));

    let mut talker_cmd = Command::new(&xrce_stress_test_binary);
    talker_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "10");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "xrce-100hz-talker")
        .expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(20))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    let received = count_pattern(&listener_output, "Received:");
    eprintln!("XRCE 100Hz test: received {} messages", received);

    assert!(
        received >= 10,
        "Expected at least 10 messages at 100Hz, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );

    drop(agent);
}

/// XRCE throughput burst test (0ms interval).
#[rstest]
fn test_xrce_throughput_burst(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    if !require_xrce_agent() {
        return;
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut listener_cmd = Command::new(&xrce_stress_test_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-burst-listener")
        .expect("Failed to start listener");

    let _ = listener.wait_for_output_pattern("Ready: listening", Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(1));

    let mut talker_cmd = Command::new(&xrce_stress_test_binary);
    talker_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "0");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "xrce-burst-talker")
        .expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(20))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    let received = count_pattern(&listener_output, "Received:");
    eprintln!("XRCE burst test: received {} messages", received);

    assert!(
        received >= 3,
        "Expected at least 3 messages in burst mode, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );

    drop(agent);
}

// =============================================================================
// QEMU Bare-Metal Large Message Test
// =============================================================================

/// Test that the QEMU bare-metal binary can publish at various sizes.
/// Requires qemu-system-arm + zenoh-pico-arm + QEMU slirp (user-mode) networking.
#[rstest]
fn test_qemu_zenoh_large_publish(qemu_large_msg_test_binary: PathBuf) {
    use nros_tests::fixtures::ManagedProcess;
    use std::process::Command;

    // This test uses QEMU slirp (user-mode) networking with port forwarding.
    // No TAP devices, bridge interfaces, or sudo required.
    // Skip if QEMU is not available.
    let qemu_available = Command::new("qemu-system-arm")
        .arg("--version")
        .output()
        .is_ok();
    if !qemu_available {
        eprintln!("Skipping test: qemu-system-arm not found");
        return;
    }

    let mut cmd = Command::new("qemu-system-arm");
    cmd.args([
        "-cpu",
        "cortex-m3",
        "-machine",
        "mps2-an385",
        "-nographic",
        "-semihosting-config",
        "enable=on,target=native",
        "-kernel",
    ]);
    cmd.arg(&qemu_large_msg_test_binary);

    let mut proc =
        ManagedProcess::spawn_command(cmd, "qemu-large-msg-test").expect("Failed to start QEMU");

    let output = proc
        .wait_for_output_pattern("All tests passed", Duration::from_secs(30))
        .unwrap_or_default();

    proc.kill();

    eprintln!("QEMU large msg test output:\n{}", output);

    // Verify all test sizes passed
    let pass_count = count_pattern(&output, "[PASS]");
    assert!(
        pass_count >= 6,
        "Expected at least 6 [PASS] markers (one per size), got {}.\nOutput:\n{}",
        pass_count,
        output,
    );
    assert!(
        !output.contains("[FAIL]"),
        "Unexpected [FAIL] in output.\nOutput:\n{}",
        output,
    );
}
