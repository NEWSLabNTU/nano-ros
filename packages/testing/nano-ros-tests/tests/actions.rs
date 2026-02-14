//! Action integration tests
//!
//! Tests for ROS 2 action communication between nros nodes.

use nano_ros_tests::fixtures::{
    ManagedProcess, ZenohRouter, action_client_binary, action_server_binary, require_zenohd,
    zenohd_unique,
};
use rstest::rstest;
use std::path::PathBuf;
use std::time::Duration;

// =============================================================================
// Action Server/Client Communication Tests
// =============================================================================

#[rstest]
fn test_action_server_starts(zenohd_unique: ZenohRouter, action_server_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&action_server_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(cmd, "native-rs-action-server")
        .expect("Failed to start action server");

    // Wait for server readiness
    match server.wait_for_output_pattern("Waiting for action", Duration::from_secs(5)) {
        Ok(_) => {
            eprintln!("[PASS] native-rs-action-server started successfully");
            return;
        }
        Err(_) => {}
    }

    // Check process is still running (didn't crash)
    if server.is_running() {
        eprintln!("[PASS] native-rs-action-server started (no marker yet)");
    } else {
        eprintln!("[FAIL] native-rs-action-server exited early");
        panic!("Action server failed to start");
    }
}

#[rstest]
fn test_action_client_starts(zenohd_unique: ZenohRouter, action_client_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&action_client_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
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

#[rstest]
fn test_action_server_client_communication(
    zenohd_unique: ZenohRouter,
    action_server_binary: PathBuf,
    action_client_binary: PathBuf,
) {
    use nano_ros_tests::count_pattern;
    use std::process::Command;

    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start action server first
    let mut server_cmd = Command::new(&action_server_binary);
    server_cmd.env("ZENOH_LOCATOR", &locator);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "native-rs-action-server")
        .expect("Failed to start action server");

    // Wait for server readiness
    if server
        .wait_for_output_pattern("Waiting for action", Duration::from_secs(5))
        .is_err()
        && !server.is_running()
    {
        eprintln!("[FAIL] Action server exited before client started");
        panic!("Action server failed");
    }

    // Start action client
    let mut client_cmd = Command::new(&action_client_binary);
    client_cmd.env("ZENOH_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-action-client")
        .expect("Failed to start action client");

    // Wait for client to complete (event-driven — Fibonacci(10) takes ~5.5s)
    let client_output = client
        .wait_for_output_pattern("finished", Duration::from_secs(20))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    // Kill server
    server.kill();

    eprintln!("Client output:\n{}", client_output);

    // Check client received goal acceptance
    let goal_accepted = client_output.contains("Goal accepted");
    eprintln!("Goal accepted: {}", goal_accepted);

    // Count feedback messages received
    let feedback_count = count_pattern(&client_output, "Feedback #");
    eprintln!("Feedback messages received: {}", feedback_count);

    // Check if action completed
    let completed = client_output.contains("action completed")
        || client_output.contains("Action client finished");
    eprintln!("Action completed: {}", completed);

    // Verify results
    if goal_accepted && feedback_count > 0 && completed {
        eprintln!("[PASS] Action server-client communication works");
        eprintln!("  - Goal was accepted");
        eprintln!("  - Received {} feedback messages", feedback_count);
        eprintln!("  - Action completed successfully");
    } else {
        eprintln!("[FAIL] Action communication incomplete");
        if !goal_accepted {
            eprintln!("  - Goal was NOT accepted");
        }
        if feedback_count == 0 {
            eprintln!("  - No feedback received");
        }
        if !completed {
            eprintln!("  - Action did not complete");
        }
        panic!(
            "Action communication failed: goal_accepted={}, feedback_count={}, completed={}",
            goal_accepted, feedback_count, completed
        );
    }
}

// =============================================================================
// Additional Tests (require modifications to examples)
// =============================================================================

// Note: The following tests require modifications to the action examples:
// - Goal cancellation: Requires --cancel-after option in client
// - Multiple concurrent goals: Requires multi-goal support in client
// - Feedback streaming: Already verified in test_action_server_client_communication

// =============================================================================
// Detection Tests
// =============================================================================

#[test]
fn test_action_binaries_exist() {
    use nano_ros_tests::fixtures::{build_native_action_client, build_native_action_server};

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
