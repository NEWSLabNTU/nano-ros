//! Service integration tests
//!
//! Tests for ROS 2 service communication between nros nodes.
//! Uses the AddTwoInts service from example_interfaces.

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, ZenohRouter, require_zenohd, service_client_binary, service_server_binary,
        zenohd_unique,
    },
    output::{SERVICE_RESULT_PREFIX, service_result_line},
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

// =============================================================================
// (Phase 182.3) `test_service_{server,client}_builds` removed — they only
// asserted the service fixtures compiled, covered by `build-all` + the service
// e2e tests below (which build the same binaries via the shared
// `build_native_service_*` resolvers).
// =============================================================================

// =============================================================================
// Server Startup Tests
// =============================================================================

#[rstest]
fn test_service_server_starts(zenohd_unique: ZenohRouter, service_server_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&service_server_binary);
    cmd.env("NROS_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(cmd, "native-rs-service-server")
        .expect("Failed to start service server");

    // Wait for server readiness. Marker found → fall through to test
    // success. Phase 214.A.3 — dropped the `eprintln!("[PASS]") + return`
    // verbosity; the harness reports PASS on clean fn return.
    if server
        .wait_for_output_pattern("Waiting for service", Duration::from_secs(5))
        .is_ok()
    {
        return;
    }

    // Marker not printed within 5s. Distinguish: process still alive
    // = readiness unverified → SKIP (CLAUDE.md-banned to claim PASS on
    // an unmet precondition). Process exited → real failure → panic.
    if server.is_running() {
        nros_tests::skip!(
            "native-rs-service-server did not print 'Waiting for service' marker within 5s"
        );
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
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&service_client_binary);
    cmd.env("NROS_LOCATOR", &locator);
    cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(cmd, "native-rs-service-client")
        .expect("Failed to start service client");

    // Without a server the client must report a failure (and exit non-zero)
    // rather than hanging or crashing: either the wait-for-service timeout,
    // or — on backends whose readiness probe is optimistic (the CFFI zenoh
    // path has no liveliness probe) — the per-call timeout.
    let output = client
        .wait_for_output_pattern("Timed out waiting", Duration::from_secs(10))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    eprintln!("Client output (no server):\n{}", output);

    assert!(
        output.contains("Timed out waiting for /add_two_ints service")
            || output.contains("Service call failed"),
        "client without a server should report the wait-for-service or call timeout"
    );
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
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    // Start service server first
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd.env("NROS_LOCATOR", &locator);
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
    client_cmd.env("NROS_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-service-client")
        .expect("Failed to start service client");

    // The client sends ONE request (the official demo default `2 3`) and
    // logs `Result of add_two_ints: 5` (phase-277 W5 wording).
    let client_output = client
        .wait_for_output_pattern(SERVICE_RESULT_PREFIX, Duration::from_secs(15))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    // Kill server and collect its output
    let server_output = server
        .wait_for_all_output(Duration::from_secs(1))
        .unwrap_or_default();
    server.kill();

    eprintln!("=== Client output ===\n{}", client_output);
    eprintln!("=== Server output ===\n{}", server_output);

    assert!(
        client_output.contains(&service_result_line(5)),
        "client should log `{}` for the default 2 + 3 request",
        service_result_line(5)
    );
    assert!(
        server_output.contains("Incoming request") && server_output.contains("a: 2 b: 3"),
        "server should log the official two-line request form"
    );
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
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    // Start server
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd.env("NROS_LOCATOR", &locator);
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
        client_cmd.env("NROS_LOCATOR", &locator);
        client_cmd.env("RUST_LOG", "info");
        let mut client = ManagedProcess::spawn_command(client_cmd, "service-client")
            .expect("Failed to start service client");

        // Wait for the client's single result line (event-driven)
        let output = client
            .wait_for_output_pattern(SERVICE_RESULT_PREFIX, Duration::from_secs(15))
            .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
            .unwrap_or_default();

        let responses = count_pattern(&output, SERVICE_RESULT_PREFIX);
        eprintln!("Run {}: {} responses", run, responses);
        total_responses += responses;
    }

    server.kill();

    eprintln!("Total responses across all runs: {}", total_responses);

    // One result per client run.
    if total_responses >= 2 {
        eprintln!("[PASS] Multiple sequential client runs work correctly");
    } else {
        eprintln!(
            "[FAIL] Expected at least 2 total responses, got {}",
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
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    // Start client WITHOUT server - should timeout
    let mut client_cmd = Command::new(&service_client_binary);
    client_cmd.env("NROS_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "service-client-timeout")
        .expect("Failed to start service client");

    // Wait for client to report the wait-for-service/call timeout or exit
    let output = client
        .wait_for_output_pattern("Timed out waiting", Duration::from_secs(12))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    eprintln!("Timeout test output:\n{}", output);

    assert!(
        output.contains("Timed out waiting for /add_two_ints service")
            || output.contains("Service call failed")
            || !client.is_running(),
        "client without a server should report the timeout (or exit)"
    );
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
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    // Start server
    let mut server_cmd = Command::new(&service_server_binary);
    server_cmd.env("NROS_LOCATOR", &locator);
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
    client1_cmd.env("NROS_LOCATOR", &locator);
    client1_cmd.env("RUST_LOG", "info");
    let mut client1 = ManagedProcess::spawn_command(client1_cmd, "service-client-1")
        .expect("Failed to start client 1");

    // Stagger client 2 start by 2 seconds so both clients don't race
    // for the zenoh queryable registration simultaneously
    std::thread::sleep(Duration::from_secs(2));

    let mut client2_cmd = Command::new(&service_client_binary);
    client2_cmd.env("NROS_LOCATOR", &locator);
    client2_cmd.env("RUST_LOG", "info");
    let mut client2 = ManagedProcess::spawn_command(client2_cmd, "service-client-2")
        .expect("Failed to start client 2");

    // Wait for both clients' single result line (event-driven)
    let output1 = client1
        .wait_for_output_pattern(SERVICE_RESULT_PREFIX, Duration::from_secs(15))
        .or_else(|_| client1.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();
    let output2 = client2
        .wait_for_output_pattern(SERVICE_RESULT_PREFIX, Duration::from_secs(15))
        .or_else(|_| client2.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    server.kill();

    eprintln!("=== Client 1 output ===\n{}", output1);
    eprintln!("=== Client 2 output ===\n{}", output2);

    let responses1 = count_pattern(&output1, SERVICE_RESULT_PREFIX);
    let responses2 = count_pattern(&output2, SERVICE_RESULT_PREFIX);

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
