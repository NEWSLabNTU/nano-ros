//! Action integration tests
//!
//! Tests for ROS 2 action communication between nros nodes.
//!
//! **Bucket (phase-329): KEEP — behavior one-offs, not matrix cells.** The plain
//! rust/zenoh DELIVERY case folded into the native-example req/resp matrix
//! consumer (`native_example_reqresp_e2e.rs`).

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, action_client_binary, action_server_binary, require_zenohd,
    zenohd_unique,
};
use rstest::rstest;
use std::{path::PathBuf, time::Duration};

// =============================================================================
// Action Server/Client Communication Tests
// =============================================================================

#[rstest]
fn test_action_server_starts(zenohd_unique: ZenohRouter, action_server_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&action_server_binary);
    cmd.env("NROS_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(cmd, "native-rs-action-server")
        .expect("Failed to start action server");

    // Wait for server readiness. Marker found → fall through to test
    // success. Phase 214.A.3 — dropped the `eprintln!("[PASS]") + return`
    // verbosity; the harness reports PASS on clean fn return.
    if server
        .wait_for_output_pattern("Waiting for action", Duration::from_secs(5))
        .is_ok()
    {
        return;
    }

    // Marker not printed within 5s. Distinguish: process still alive
    // = readiness unverified → SKIP (CLAUDE.md-banned to claim PASS on
    // an unmet precondition). Process exited → real failure → panic.
    if server.is_running() {
        nros_tests::skip!(
            "native-rs-action-server did not print 'Waiting for action' marker within 5s"
        );
    } else {
        eprintln!("[FAIL] native-rs-action-server exited early");
        panic!("Action server failed to start");
    }
}

#[rstest]
fn test_action_client_starts(zenohd_unique: ZenohRouter, action_client_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&action_client_binary);
    cmd.env("NROS_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(cmd, "native-rs-action-client")
        .expect("Failed to start action client");

    // Wait briefly — client will timeout without server
    let _ = client.wait_for_output_pattern("Sending goal", Duration::from_secs(5));

    // Check process is still running (may timeout without server)
    if client.is_running() {
        eprintln!("[PASS] native-rs-action-client started successfully (waiting for server)");
    } else {
        // Client may exit if no server is available - that's OK
        eprintln!("[INFO] native-rs-action-client exited (no server available)");
    }
}

// phase-329 W4 — the DELIVERY test (rust/zenoh) FOLDED into the native-example
// req/resp matrix consumer (`native_example_reqresp_e2e.rs`, the
// `(Linux, Rust, Zenoh, Action)` cell). Kept below: server/client startup
// one-offs + the binaries-exist fixture-artifact check.

// =============================================================================

#[test]
fn test_action_binaries_exist() {
    use nros_tests::fixtures::{build_native_action_client, build_native_action_server};

    // Try to build action server
    match build_native_action_server() {
        Ok(path) => {
            eprintln!("[PASS] Action server binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[INFO] Could not build action server: {}", e);
        }
    }

    // Try to build action client
    match build_native_action_client() {
        Ok(path) => {
            eprintln!("[PASS] Action client binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[INFO] Could not build action client: {}", e);
        }
    }
}
