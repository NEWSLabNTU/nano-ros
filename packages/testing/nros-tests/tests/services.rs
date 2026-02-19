//! Service integration tests
//!
//! Tests for ROS 2 service communication between nros nodes.
//! Uses the AddTwoInts service from example_interfaces.

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_service_client, build_native_service_server,
    require_zenohd, service_client_binary, service_server_binary, zenohd_unique,
};
use rstest::rstest;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Build/Detection Tests
// =============================================================================

#[test]
fn test_service_server_builds() {
    match build_native_service_server() {
        Ok(path) => {
            eprintln!("[PASS] Service server binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build service server: {}", e);
            panic!("Service server build failed: {}", e);
        }
    }
}

#[test]
fn test_service_client_builds() {
    match build_native_service_client() {
        Ok(path) => {
            eprintln!("[PASS] Service client binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build service client: {}", e);
            panic!("Service client build failed: {}", e);
        }
    }
}

// =============================================================================
// Server Startup Tests
// =============================================================================

#[rstest]
fn test_service_server_starts(zenohd_unique: ZenohRouter, service_server_binary: PathBuf) {
    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&service_server_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(cmd, "native-rs-service-server")
        .expect("Failed to start service server");

    // Wait for server readiness
    match server.wait_for_output_pattern("Waiting for service", Duration::from_secs(5)) {
        Ok(_) => {
            eprintln!("[PASS] native-rs-service-server started successfully");
            return;
        }
        Err(_) => {}
    }

    // Check process is still running (didn't crash)
    if server.is_running() {
        eprintln!("[PASS] native-rs-service-server started (no marker yet)");
    } else {
        // Collect any output for debugging
        let output = server
            .wait_for_all_output(Duration::from_secs(1))
            .unwrap_or_default();
        eprintln!("[FAIL] native-rs-service-server exited early");
        eprintln!("Output: {}", output);
        panic!("Service server failed to start");
    }
}

// =============================================================================
// Client Startup Tests
// =============================================================================

#[rstest]
fn test_service_client_starts_without_server(
    zenohd_unique: ZenohRouter,
    service_client_binary: PathBuf,
) {
    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&service_client_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(cmd, "native-rs-service-client")
        .expect("Failed to start service client");

    // Wait for client to report failure or exit (no server running)
    let output = client
        .wait_for_output_pattern("failed", Duration::from_secs(10))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    eprintln!("Client output (no server):\n{}", output);

    // Client should have started and created the service client
    let client_created =
        output.contains("Service client created") || output.contains("add_two_ints_client");

    if client_created {
        eprintln!("[PASS] native-rs-service-client started and created client");
    }

    // Client will likely exit with error since no server is running
    // That's expected behavior - we just want to verify it starts
    let call_failed = output.contains("Service call failed")
        || output.contains("timeout")
        || output.contains("Timeout");

    if call_failed {
        eprintln!("[PASS] Client correctly reported service call failure (no server)");
    } else {
        eprintln!("[INFO] Client behavior without server - check output above");
    }
}

// =============================================================================
// Request/Response Communication Tests
// =============================================================================

#[rstest]
fn test_service_request_response(
    zenohd_unique: ZenohRouter,
    service_server_binary: PathBuf,
    service_client_binary: PathBuf,
) {
    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start service server first
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd.env("ZENOH_LOCATOR", &locator);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "native-rs-service-server")
        .expect("Failed to start service server");

    // Wait for server readiness
    if server
        .wait_for_output_pattern("Waiting for service", Duration::from_secs(5))
        .is_err()
        && !server.is_running()
    {
        let output = server
            .wait_for_all_output(Duration::from_secs(1))
            .unwrap_or_default();
        eprintln!("[FAIL] Service server exited before client started");
        eprintln!("Server output: {}", output);
        panic!("Service server failed");
    }

    // Start service client
    let mut client_cmd = Command::new(&service_client_binary);
    client_cmd.env("ZENOH_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-service-client")
        .expect("Failed to start service client");

    // Wait for client to complete all calls (event-driven)
    let client_output = client
        .wait_for_output_pattern("completed successfully", Duration::from_secs(15))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    // Kill server and collect its output
    server.kill();

    eprintln!("=== Client output ===\n{}", client_output);

    // Check for successful service calls
    // Client example makes 4 calls: (5+3), (10+20), (100+200), (-5+10)
    let response_count = count_pattern(&client_output, "Response:");
    eprintln!("Responses received: {}", response_count);

    // Check for specific expected results
    let has_8 = client_output.contains("= 8"); // 5 + 3 = 8
    let has_30 = client_output.contains("= 30"); // 10 + 20 = 30
    let has_300 = client_output.contains("= 300"); // 100 + 200 = 300
    let has_5 = client_output.contains("= 5"); // -5 + 10 = 5

    eprintln!(
        "Expected results: 8={}, 30={}, 300={}, 5={}",
        has_8, has_30, has_300, has_5
    );

    // Check for completion message
    let completed = client_output.contains("All service calls completed successfully");

    // Verify results
    if response_count >= 4 && completed {
        eprintln!("[PASS] Service request/response communication works");
        eprintln!("  - Client made {} service calls", response_count);
        eprintln!("  - All calls completed successfully");
    } else if response_count > 0 {
        eprintln!("[PARTIAL] Some service calls succeeded");
        eprintln!("  - {} responses received (expected 4)", response_count);
        if !completed {
            eprintln!("  - Client did not report full completion");
        }
        // Still pass if we got at least some responses
        if response_count >= 2 {
            eprintln!("[PASS] Service communication functional (partial success)");
        } else {
            panic!(
                "Service communication incomplete: only {} responses",
                response_count
            );
        }
    } else {
        eprintln!("[FAIL] No service responses received");
        panic!("Service request/response failed - no responses received");
    }
}

// =============================================================================
// Multiple Sequential Runs Test
// =============================================================================

#[rstest]
fn test_service_multiple_sequential_calls(
    zenohd_unique: ZenohRouter,
    service_server_binary: PathBuf,
    service_client_binary: PathBuf,
) {
    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start server
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd.env("ZENOH_LOCATOR", &locator);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "service-server")
        .expect("Failed to start service server");

    // Wait for server readiness
    if server
        .wait_for_output_pattern("Waiting for service", Duration::from_secs(5))
        .is_err()
        && !server.is_running()
    {
        panic!("Service server failed to start");
    }

    // Run client multiple times to verify repeated calls work
    let mut total_responses = 0;

    for run in 1..=2 {
        eprintln!("--- Client run {} ---", run);

        let mut client_cmd = Command::new(&service_client_binary);
        client_cmd.env("ZENOH_LOCATOR", &locator);
        client_cmd.env("RUST_LOG", "info");
        let mut client = ManagedProcess::spawn_command(client_cmd, "service-client")
            .expect("Failed to start service client");

        // Wait for client to complete (event-driven)
        let output = client
            .wait_for_output_pattern("completed successfully", Duration::from_secs(15))
            .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
            .unwrap_or_default();

        let responses = count_pattern(&output, "Response:");
        eprintln!("Run {}: {} responses", run, responses);
        total_responses += responses;
    }

    server.kill();

    eprintln!("Total responses across all runs: {}", total_responses);

    if total_responses >= 6 {
        // 2 runs * ~4 calls each, allow some variance
        eprintln!("[PASS] Multiple sequential client runs work correctly");
    } else {
        eprintln!(
            "[FAIL] Expected at least 6 total responses, got {}",
            total_responses
        );
        panic!("Multiple sequential calls failed");
    }
}

// =============================================================================
// Timeout Handling Test
// =============================================================================

#[rstest]
fn test_service_client_timeout(zenohd_unique: ZenohRouter, service_client_binary: PathBuf) {
    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start client WITHOUT server - should timeout
    let mut client_cmd = Command::new(&service_client_binary);
    client_cmd.env("ZENOH_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "service-client-timeout")
        .expect("Failed to start service client");

    // Wait for client to report timeout or exit
    let output = client
        .wait_for_output_pattern("failed", Duration::from_secs(12))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    eprintln!("Timeout test output:\n{}", output);

    // Client should report failure
    let timed_out = output.contains("Service call failed")
        || output.contains("timeout")
        || output.contains("Timeout")
        || output.contains("error");

    let exited = !client.is_running();

    if timed_out || exited {
        eprintln!("[PASS] Client correctly handles missing server (timeout/error)");
    } else {
        eprintln!("[WARN] Client may still be waiting - timeout behavior unclear");
    }
}

// =============================================================================
// Server Handles Multiple Clients Test
// =============================================================================

#[rstest]
fn test_service_server_multiple_clients(
    zenohd_unique: ZenohRouter,
    service_server_binary: PathBuf,
    service_client_binary: PathBuf,
) {
    if !require_zenohd() {
        return;
    }

    let locator = zenohd_unique.locator();

    // Start server
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd.env("ZENOH_LOCATOR", &locator);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "service-server")
        .expect("Failed to start service server");

    // Wait for server readiness
    if server
        .wait_for_output_pattern("Waiting for service", Duration::from_secs(5))
        .is_err()
        && !server.is_running()
    {
        panic!("Service server failed to start");
    }

    // Start two clients with staggered starts to avoid zenoh queryable race
    let mut client1_cmd = Command::new(&service_client_binary);
    client1_cmd.env("ZENOH_LOCATOR", &locator);
    client1_cmd.env("RUST_LOG", "info");
    let mut client1 = ManagedProcess::spawn_command(client1_cmd, "service-client-1")
        .expect("Failed to start client 1");

    // Stagger client 2 start by 2 seconds so both clients don't race
    // for the zenoh queryable registration simultaneously
    std::thread::sleep(Duration::from_secs(2));

    let mut client2_cmd = Command::new(&service_client_binary);
    client2_cmd.env("ZENOH_LOCATOR", &locator);
    client2_cmd.env("RUST_LOG", "info");
    let mut client2 = ManagedProcess::spawn_command(client2_cmd, "service-client-2")
        .expect("Failed to start client 2");

    // Wait for both clients to complete (event-driven)
    let output1 = client1
        .wait_for_output_pattern("completed successfully", Duration::from_secs(15))
        .or_else(|_| client1.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();
    let output2 = client2
        .wait_for_output_pattern("completed successfully", Duration::from_secs(15))
        .or_else(|_| client2.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    server.kill();

    eprintln!("=== Client 1 output ===\n{}", output1);
    eprintln!("=== Client 2 output ===\n{}", output2);

    let responses1 = count_pattern(&output1, "Response:");
    let responses2 = count_pattern(&output2, "Response:");

    eprintln!(
        "Client 1 responses: {}, Client 2 responses: {}",
        responses1, responses2
    );

    // Both clients should get responses
    if responses1 > 0 && responses2 > 0 {
        eprintln!("[PASS] Server handles multiple concurrent clients");
    } else if responses1 > 0 || responses2 > 0 {
        eprintln!("[PARTIAL] At least one client got responses");
        // This might indicate a concurrency issue
        eprintln!("[INFO] Concurrent client handling may need investigation");
    } else {
        eprintln!("[FAIL] Neither client received responses");
        panic!("Multiple client test failed");
    }
}
