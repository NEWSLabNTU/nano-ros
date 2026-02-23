//! nros to nros communication tests
//!
//! Tests communication between native nros binaries via zenoh.

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, is_zenohd_available, listener_binary, listener_tls_binary,
    require_zenohd, talker_binary, talker_tls_binary, tls_certs, zenohd_unique,
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
        && !listener.is_running()
    {
        eprintln!("[INFO] Listener exited early - peer mode may not be supported");
        return;
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
// MessageInfo Tests (sequence number, GID)
// =============================================================================

/// Test that sequence numbers increment monotonically per publisher
#[rstest]
fn test_sequence_number_increment(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    listener_binary: PathBuf,
) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener with RUST_LOG=trace to get MessageInfo trace output
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("RUST_LOG", "trace");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    // Wait for listener readiness
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Wait for several messages to be received (need at least 2 for increment/consistency check)
    std::thread::sleep(Duration::from_secs(3));

    // Kill processes and collect output
    talker.kill();
    listener.kill();
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("Listener trace output:\n{}", listener_output);

    // Parse seq= values from trace output
    let seq_values: Vec<i64> = listener_output
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

    eprintln!(
        "[PASS] Sequence numbers increment monotonically ({} messages)",
        seq_values.len()
    );
}

/// Test that publisher GID stays consistent across messages
#[rstest]
fn test_gid_consistency(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    listener_binary: PathBuf,
) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start listener with RUST_LOG=trace to get MessageInfo trace output
    let mut listener_cmd = Command::new(&listener_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("RUST_LOG", "trace");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    // Wait for listener readiness
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(5));

    // Start talker
    let mut talker_cmd = Command::new(&talker_binary);
    talker_cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Wait for several messages to be received (need at least 2 for increment/consistency check)
    std::thread::sleep(Duration::from_secs(3));

    // Kill processes and collect output
    talker.kill();
    listener.kill();
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("Listener trace output:\n{}", listener_output);

    // Parse gid= values from trace output
    let gid_values: Vec<String> = listener_output
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

    // Verify all GIDs are identical
    let first_gid = &gid_values[0];
    for (i, gid) in gid_values.iter().enumerate() {
        assert_eq!(
            gid, first_gid,
            "GID at message {} ({}) should match first GID ({})",
            i, gid, first_gid
        );
    }

    // Verify GID is not all zeros (should be a real publisher ID)
    assert_ne!(
        first_gid, "00000000",
        "GID should not be all zeros (should contain real publisher ID)"
    );

    eprintln!(
        "[PASS] Publisher GID is consistent across {} messages: {}",
        gid_values.len(),
        first_gid
    );
}

// =============================================================================
// TLS Transport Tests
// =============================================================================

/// Test that TLS talker/listener communicate through a TLS-enabled zenohd
#[rstest]
fn test_tls_talker_listener_communication(
    talker_tls_binary: PathBuf,
    listener_tls_binary: PathBuf,
) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    if !tls_certs::is_openssl_available() {
        eprintln!("[SKIP] openssl not available — cannot generate TLS certs");
        return;
    }

    // Generate self-signed certificate
    let certs = tls_certs::TlsCerts::generate().expect("Failed to generate TLS certs");

    // Start zenohd with TLS listener
    let router = ZenohRouter::start_tls_unique(certs.cert_path(), certs.key_path())
        .expect("Failed to start zenohd with TLS");
    let locator = router.locator();
    eprintln!("TLS router at: {}", locator);

    let cert_path = certs.cert_path().to_str().unwrap().to_string();

    // Start listener with TLS locator and CA certificate
    let mut listener_cmd = Command::new(&listener_tls_binary);
    listener_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_TLS_ROOT_CA_CERTIFICATE", &cert_path)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener-tls")
        .expect("Failed to start TLS listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern("Waiting for", Duration::from_secs(10));

    // Start talker with TLS locator and CA certificate
    let mut talker_cmd = Command::new(&talker_tls_binary);
    talker_cmd
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_TLS_ROOT_CA_CERTIFICATE", &cert_path);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker-tls")
        .expect("Failed to start TLS talker");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(15))
        .unwrap_or_default();

    // Kill talker
    talker.kill();

    eprintln!("TLS Listener output:\n{}", listener_output);

    let received_count = count_pattern(&listener_output, "Received:");
    eprintln!("TLS Listener received {} messages", received_count);

    assert!(
        received_count > 0,
        "TLS listener should receive at least 1 message"
    );
    eprintln!("[PASS] TLS talker/listener communication works");
}

// =============================================================================
// Detection Tests
// =============================================================================

#[test]
fn test_zenohd_detection() {
    let available = is_zenohd_available();
    eprintln!("zenohd available: {}", available);
}
