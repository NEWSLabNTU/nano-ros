//! ThreadX Linux integration tests
//!
//! Tests that verify ThreadX Linux examples build and run natively.
//! ThreadX Linux examples use the ThreadX Linux simulation port with
//! a NetX Duo TAP network driver (`/dev/net/tun`) attached to persistent
//! user-owned TAP interfaces created by `just setup-network`.
//!
//! Prerequisites:
//! - `THREADX_DIR` env var pointing to ThreadX source (e.g., `third-party/threadx/kernel`)
//! - `NETX_DIR` env var pointing to NetX Duo source (e.g., `third-party/threadx/netxduo`)
//!
//! Run with: `just test-threadx-linux`
//! Or: `cargo nextest run -p nros-tests --test threadx_linux`

use nros_tests::count_pattern;
use nros_tests::fixtures::threadx_linux::{
    build_threadx_action_client, build_threadx_action_server, build_threadx_c_action_client,
    build_threadx_c_action_server, build_threadx_c_listener, build_threadx_c_service_client,
    build_threadx_c_service_server, build_threadx_c_talker, build_threadx_cpp_listener,
    build_threadx_cpp_service_client, build_threadx_cpp_service_server, build_threadx_cpp_talker,
    build_threadx_listener, build_threadx_service_client, build_threadx_service_server,
    build_threadx_talker, is_cmake_available, is_nsos_netx_available, is_threadx_available,
};
use nros_tests::fixtures::{ZenohRouter, is_zenohd_available, require_zenohd};
use nros_tests::platform;
use nros_tests::process::{ManagedProcess, kill_process_group};
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Skip test if ThreadX build prerequisites are not available
///
/// ThreadX-Linux uses NSOS-style networking (nsos-netx forwards
/// `nx_bsd_*` calls to host POSIX sockets), so no NetX Duo source
/// or TAP/veth interface is needed for the build.
fn require_threadx() -> bool {
    if !is_threadx_available() {
        eprintln!("Skipping test: THREADX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_nsos_netx_available() {
        eprintln!("Skipping test: nsos-netx not found at packages/drivers/nsos-netx/");
        return false;
    }
    true
}

/// Skip test if full ThreadX E2E prerequisites are not available
///
/// E2E tests require:
/// 1. ThreadX build prerequisites (THREADX_DIR + nsos-netx)
/// 2. zenohd router (built from submodule)
///
/// No bridge / TAP / veth setup needed — nsos-netx uses host loopback.
fn require_threadx_e2e() -> bool {
    if !require_threadx() {
        return false;
    }
    if !require_zenohd() {
        return false;
    }
    true
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_threadx_detection() {
    let threadx = is_threadx_available();
    let nsos_netx = is_nsos_netx_available();
    let zenohd = is_zenohd_available();
    eprintln!("ThreadX available: {}", threadx);
    eprintln!("nsos-netx available: {}", nsos_netx);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// Build tests (require THREADX_DIR + nsos-netx)
// =============================================================================
#[test]
fn test_threadx_all_examples_build() {
    if !require_threadx() {
        nros_tests::skip!("require_threadx check failed");
    }

    let results = [
        ("talker", build_threadx_talker()),
        ("listener", build_threadx_listener()),
        ("service-server", build_threadx_service_server()),
        ("service-client", build_threadx_service_client()),
        ("action-server", build_threadx_action_server()),
        ("action-client", build_threadx_action_client()),
    ];

    let mut all_ok = true;
    for (name, result) in &results {
        match result {
            Ok(path) => eprintln!("  OK: {} -> {}", name, path.display()),
            Err(e) => {
                eprintln!("  FAIL: {} -> {:?}", name, e);
                all_ok = false;
            }
        }
    }

    assert!(all_ok, "Not all ThreadX Linux examples built successfully");
}

// =============================================================================
// E2E Network tests (require TAP bridge + zenohd)
// =============================================================================
//
// ThreadX Linux examples are native binaries that use the ThreadX Linux
// simulation port. They communicate over TAP interfaces via NetX Duo's
// raw-socket Linux driver.
//
// Network topology:
//   ThreadX node 0 (tap-qemu0, 192.0.3.10) --+
//                                              |-- Bridge (qemu-br, 192.0.3.1) -- zenohd
//   ThreadX node 1 (tap-qemu1, 192.0.3.11) --+
//
// Prerequisites:
//   1. TAP bridge: sudo ./scripts/qemu/setup-network.sh
//   2. zenohd: just build-zenohd
//   3. Run: just test-threadx-linux

/// Test pub/sub message exchange between ThreadX Linux instances.
///
/// Launches a listener and a talker as native processes, verifies
/// that the listener receives Int32 messages published by the talker.
#[test]
fn test_threadx_pubsub_e2e() {
    if !require_threadx_e2e() {
        nros_tests::skip!("require_threadx_e2e check failed");
    }

    // Build both binaries
    let talker_bin = build_threadx_talker().expect("Failed to build talker");
    let listener_bin = build_threadx_listener().expect("Failed to build listener");

    // Start zenohd (firmware hardcodes tcp/192.0.3.1:<port>)
    let _zenohd = ZenohRouter::start_on("0.0.0.0", platform::THREADX_LINUX.zenohd_port)
        .expect("Failed to start zenohd");

    // Start listener first (subscriber before publisher)
    eprintln!("Starting listener on tap-qemu1...");
    let mut listener = ManagedProcess::spawn(listener_bin, &[], "threadx-linux-listener")
        .expect("Failed to start listener");

    // Stabilization delay: ThreadX boot + NetX init + zenoh connect (~5s)
    std::thread::sleep(Duration::from_secs(5));

    // Start talker
    eprintln!("Starting talker on tap-qemu0...");
    let mut talker = ManagedProcess::spawn(talker_bin, &[], "threadx-linux-talker")
        .expect("Failed to start talker");

    // Wait for listener to complete (capture stdout+stderr for error diagnostics)
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to finish publishing
    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(15))
        .unwrap_or_default();

    kill_process_group(talker.handle_mut());
    kill_process_group(listener.handle_mut());

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify listener booted and received messages
    if !listener_output.contains("Waiting for messages") {
        panic!(
            "ThreadX pubsub E2E failed — listener did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - TAP bridge: `ip addr show qemu-br` (should have 192.0.3.1/24)\n\
             - veth devices: `ip link show veth-tx0 veth-tx1` (should be UP)\n\
             - zenohd reachable: bridge IP 192.0.3.1:7447"
        );
    }

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("Messages received: {}", received_count);

    if received_count > 0 {
        eprintln!(
            "[PASS] ThreadX pubsub E2E: {} messages exchanged",
            received_count
        );
    } else {
        panic!("ThreadX pubsub E2E failed — listener received 0 messages");
    }
}

/// Test service request/response between ThreadX Linux instances.
///
/// Launches a service server and a client as native processes,
/// verifies that the client receives correct AddTwoInts responses.
#[test]
fn test_threadx_service_e2e() {
    if !require_threadx_e2e() {
        nros_tests::skip!("require_threadx_e2e check failed");
    }

    let server_bin = build_threadx_service_server().expect("Failed to build service server");
    let client_bin = build_threadx_service_client().expect("Failed to build service client");

    let _zenohd = ZenohRouter::start_on("0.0.0.0", platform::THREADX_LINUX.zenohd_port)
        .expect("Failed to start zenohd");

    // Start server first
    eprintln!("Starting service server on tap-qemu0...");
    let mut server = ManagedProcess::spawn(server_bin, &[], "threadx-linux-service-server")
        .expect("Failed to start server");

    // Stabilization delay: ThreadX boot + NetX init + zenoh connect (~5s)
    std::thread::sleep(Duration::from_secs(5));

    // Start client
    eprintln!("Starting service client on tap-qemu1...");
    let mut client = ManagedProcess::spawn(client_bin, &[], "threadx-linux-service-client")
        .expect("Failed to start client");

    // Stabilization delay for client to discover server's service queryable
    std::thread::sleep(Duration::from_secs(5));

    // Wait for client to complete all service calls (4 calls: 5+3, 10+20, 100+200, -5+10)
    let client_output = client
        .wait_for_all_output(Duration::from_secs(60))
        .unwrap_or_default();

    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    kill_process_group(server.handle_mut());
    kill_process_group(client.handle_mut());

    eprintln!("Server output:\n{}", server_output);
    eprintln!("Client output:\n{}", client_output);

    // Check for successful responses
    let response_count = count_pattern(&client_output, "Response:");
    let completed = client_output.contains("All service calls completed");

    // Verify specific results
    let has_8 = client_output.contains("= 8"); // 5 + 3
    let has_30 = client_output.contains("= 30"); // 10 + 20
    let has_300 = client_output.contains("= 300"); // 100 + 200

    eprintln!("Responses: {}, completed: {}", response_count, completed);
    eprintln!("Results: 8={}, 30={}, 300={}", has_8, has_30, has_300);

    if response_count >= 4 && completed {
        eprintln!(
            "[PASS] ThreadX service E2E: {} responses, all correct",
            response_count
        );
    } else if response_count > 0 {
        eprintln!(
            "[PARTIAL] ThreadX service E2E: {} of 4 responses",
            response_count
        );
    } else if !client_output.contains("Service client ready")
        && !client_output.contains("nros ThreadX Linux Platform")
    {
        panic!(
            "ThreadX service E2E failed — client did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - TAP bridge: `ip addr show qemu-br` (should have 192.0.3.1/24)\n\
             - veth devices: `ip link show veth-tx0 veth-tx1` (should be UP)\n\
             - zenohd reachable: bridge IP 192.0.3.1:7447"
        );
    } else {
        panic!(
            "ThreadX service E2E failed — client received 0 responses.\n\
             Client reached readiness but no service replies were received.\n\
             This may indicate a zenoh queryable discovery timeout."
        );
    }
}

/// Test action goal/feedback/result between ThreadX Linux instances.
///
/// Launches an action server and a client as native processes,
/// verifies that the client receives Fibonacci feedback and final result.
#[test]
fn test_threadx_action_e2e() {
    if !require_threadx_e2e() {
        nros_tests::skip!("require_threadx_e2e check failed");
    }

    let server_bin = build_threadx_action_server().expect("Failed to build action server");
    let client_bin = build_threadx_action_client().expect("Failed to build action client");

    let _zenohd = ZenohRouter::start_on("0.0.0.0", platform::THREADX_LINUX.zenohd_port)
        .expect("Failed to start zenohd");

    // Start action server first
    eprintln!("Starting action server on tap-qemu0...");
    let mut server = ManagedProcess::spawn(server_bin, &[], "threadx-linux-action-server")
        .expect("Failed to start server");

    // Stabilization delay: ThreadX boot + NetX init + zenoh connect (~5s)
    std::thread::sleep(Duration::from_secs(5));

    // Start action client
    eprintln!("Starting action client on tap-qemu1...");
    let mut client = ManagedProcess::spawn(client_bin, &[], "threadx-linux-action-client")
        .expect("Failed to start client");

    // Stabilization delay for client to discover server's action queryables
    std::thread::sleep(Duration::from_secs(5));

    // Wait for client to complete (capture stdout+stderr for error diagnostics)
    let client_output = client
        .wait_for_all_output(Duration::from_secs(60))
        .unwrap_or_default();

    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    kill_process_group(server.handle_mut());
    kill_process_group(client.handle_mut());

    eprintln!("Server output:\n{}", server_output);
    eprintln!("Client output:\n{}", client_output);

    // Verify action protocol
    let goal_accepted = client_output.contains("Goal accepted");
    let completed = client_output.contains("Action completed successfully");

    eprintln!("Goal accepted: {}, completed: {}", goal_accepted, completed);

    if goal_accepted && completed {
        eprintln!("[PASS] ThreadX action E2E: goal accepted, completed");
    } else if !client_output.contains("nros ThreadX Linux Platform")
        && !client_output.contains("Action client ready")
        && !goal_accepted
    {
        panic!(
            "ThreadX action E2E failed — client did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - TAP bridge: `ip addr show qemu-br` (should have 192.0.3.1/24)\n\
             - veth devices: `ip link show veth-tx0 veth-tx1` (should be UP)\n\
             - zenohd reachable: bridge IP 192.0.3.1:7447"
        );
    } else {
        eprintln!("[FAIL] ThreadX action E2E:");
        if !goal_accepted {
            eprintln!("  - Goal was NOT accepted");
        }
        if !completed {
            eprintln!("  - Action did not complete");
        }
        panic!(
            "ThreadX action E2E failed: accepted={}, completed={}",
            goal_accepted, completed
        );
    }
}

// =============================================================================
// C++ test helpers
// =============================================================================

/// Skip test if C++ ThreadX prerequisites are not available
fn require_threadx_cpp() -> bool {
    if !require_threadx() {
        return false;
    }
    if !is_cmake_available() {
        eprintln!("Skipping test: cmake not found");
        return false;
    }
    true
}

fn require_threadx_cpp_e2e() -> bool {
    if !require_threadx_cpp() {
        return false;
    }
    if !require_zenohd() {
        return false;
    }
    true
}

// =============================================================================
// C++ Build tests
// =============================================================================
// =============================================================================
// C++ E2E Network tests
// =============================================================================

#[test]
fn test_threadx_cpp_pubsub_e2e() {
    if !require_threadx_cpp_e2e() {
        nros_tests::skip!("require_threadx_cpp_e2e check failed");
    }

    let talker_bin = build_threadx_cpp_talker().expect("Failed to build C++ talker");
    let listener_bin = build_threadx_cpp_listener().expect("Failed to build C++ listener");

    let _zenohd = ZenohRouter::start(7455).expect("Failed to start zenohd on port 7455");

    eprintln!("Starting C++ listener...");
    let mut listener = ManagedProcess::spawn(listener_bin, &[], "threadx-cpp-listener")
        .expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting C++ talker...");
    let mut talker = ManagedProcess::spawn(talker_bin, &[], "threadx-cpp-talker")
        .expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    kill_process_group(talker.handle_mut());
    kill_process_group(listener.handle_mut());

    eprintln!("C++ Listener output:\n{}", listener_output);
    eprintln!("C++ Talker output:\n{}", talker_output);

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("C++ messages received: {}", received_count);
    assert!(
        received_count > 0,
        "ThreadX C++ pubsub E2E failed — listener received 0 messages"
    );
    eprintln!("[PASS] ThreadX C++ pubsub E2E: {} messages", received_count);
}

#[test]
fn test_threadx_cpp_service_e2e() {
    if !require_threadx_cpp_e2e() {
        nros_tests::skip!("require_threadx_cpp_e2e check failed");
    }

    let server_bin =
        build_threadx_cpp_service_server().expect("Failed to build C++ service server");
    let client_bin =
        build_threadx_cpp_service_client().expect("Failed to build C++ service client");

    let _zenohd = ZenohRouter::start(7455).expect("Failed to start zenohd on port 7455");

    eprintln!("Starting C++ service server...");
    let mut server = ManagedProcess::spawn(server_bin, &[], "threadx-cpp-server")
        .expect("Failed to start server");

    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting C++ service client...");
    let mut client = ManagedProcess::spawn(client_bin, &[], "threadx-cpp-client")
        .expect("Failed to start client");

    std::thread::sleep(Duration::from_secs(10));

    let client_output = client
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();

    kill_process_group(server.handle_mut());
    kill_process_group(client.handle_mut());

    eprintln!("C++ Client output:\n{}", client_output);

    let response_count = count_pattern(&client_output, "Response:");
    let completed = client_output.contains("All service calls completed");
    eprintln!(
        "C++ Responses: {}, completed: {}",
        response_count, completed
    );

    assert!(
        response_count > 0,
        "ThreadX C++ service E2E failed — 0 responses"
    );
    eprintln!(
        "[PASS] ThreadX C++ service E2E: {} responses",
        response_count
    );
}

// =============================================================================
// C test helpers
// =============================================================================

/// Skip test if C ThreadX prerequisites are not available
fn require_threadx_c() -> bool {
    if !require_threadx() {
        return false;
    }
    if !is_cmake_available() {
        eprintln!("Skipping test: cmake not found");
        return false;
    }
    true
}

fn require_threadx_c_e2e() -> bool {
    if !require_threadx_c() {
        return false;
    }
    if !require_zenohd() {
        return false;
    }
    true
}

// =============================================================================
// C Build tests
// =============================================================================
// =============================================================================
// C E2E Network tests
// =============================================================================

#[test]
fn test_threadx_c_pubsub_e2e() {
    if !require_threadx_c_e2e() {
        nros_tests::skip!("require_threadx_c_e2e check failed");
    }

    let talker_bin = build_threadx_c_talker().expect("Failed to build C talker");
    let listener_bin = build_threadx_c_listener().expect("Failed to build C listener");

    let _zenohd = ZenohRouter::start_on("0.0.0.0", platform::THREADX_LINUX.zenohd_port)
        .expect("Failed to start zenohd");

    eprintln!("Starting C listener...");
    let mut listener = ManagedProcess::spawn(listener_bin, &[], "threadx-c-listener")
        .expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting C talker...");
    let mut talker =
        ManagedProcess::spawn(talker_bin, &[], "threadx-c-talker").expect("Failed to start talker");

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(60))
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(15))
        .unwrap_or_default();

    kill_process_group(talker.handle_mut());
    kill_process_group(listener.handle_mut());

    eprintln!("C Listener output:\n{}", listener_output);
    eprintln!("C Talker output:\n{}", talker_output);

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("C messages received: {}", received_count);
    assert!(
        received_count > 0,
        "ThreadX C pubsub E2E failed — listener received 0 messages"
    );
    eprintln!("[PASS] ThreadX C pubsub E2E: {} messages", received_count);
}

#[test]
fn test_threadx_c_service_e2e() {
    if !require_threadx_c_e2e() {
        nros_tests::skip!("require_threadx_c_e2e check failed");
    }

    let server_bin = build_threadx_c_service_server().expect("Failed to build C service server");
    let client_bin = build_threadx_c_service_client().expect("Failed to build C service client");

    let _zenohd = ZenohRouter::start_on("0.0.0.0", platform::THREADX_LINUX.zenohd_port)
        .expect("Failed to start zenohd");

    eprintln!("Starting C service server...");
    let mut server =
        ManagedProcess::spawn(server_bin, &[], "threadx-c-server").expect("Failed to start server");

    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting C service client...");
    let mut client =
        ManagedProcess::spawn(client_bin, &[], "threadx-c-client").expect("Failed to start client");

    std::thread::sleep(Duration::from_secs(10));

    let client_output = client
        .wait_for_all_output(Duration::from_secs(60))
        .unwrap_or_default();

    kill_process_group(server.handle_mut());
    kill_process_group(client.handle_mut());

    eprintln!("C Client output:\n{}", client_output);

    let response_count = count_pattern(&client_output, "Response:");
    let completed = client_output.contains("All service calls completed");
    eprintln!("C Responses: {}, completed: {}", response_count, completed);

    assert!(
        response_count > 0,
        "ThreadX C service E2E failed — 0 responses"
    );
    eprintln!("[PASS] ThreadX C service E2E: {} responses", response_count);
}

#[test]
fn test_threadx_c_action_e2e() {
    if !require_threadx_c_e2e() {
        nros_tests::skip!("require_threadx_c_e2e check failed");
    }

    let server_bin = build_threadx_c_action_server().expect("Failed to build C action server");
    let client_bin = build_threadx_c_action_client().expect("Failed to build C action client");

    let _zenohd = ZenohRouter::start_on("0.0.0.0", platform::THREADX_LINUX.zenohd_port)
        .expect("Failed to start zenohd");

    eprintln!("Starting C action server...");
    let mut server = ManagedProcess::spawn(server_bin, &[], "threadx-c-action-server")
        .expect("Failed to start server");

    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting C action client...");
    let mut client = ManagedProcess::spawn(client_bin, &[], "threadx-c-action-client")
        .expect("Failed to start client");

    std::thread::sleep(Duration::from_secs(5));

    let client_output = client
        .wait_for_all_output(Duration::from_secs(60))
        .unwrap_or_default();

    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    kill_process_group(server.handle_mut());
    kill_process_group(client.handle_mut());

    eprintln!("C Server output:\n{}", server_output);
    eprintln!("C Client output:\n{}", client_output);

    let goal_accepted = client_output.contains("Goal accepted");
    let completed = client_output.contains("Action completed successfully");

    eprintln!("Goal accepted: {}, completed: {}", goal_accepted, completed);

    if goal_accepted && completed {
        eprintln!("[PASS] ThreadX C action E2E: goal accepted, completed");
    } else {
        eprintln!("[FAIL] ThreadX C action E2E:");
        if !goal_accepted {
            eprintln!("  - Goal was NOT accepted");
        }
        if !completed {
            eprintln!("  - Action did not complete");
        }
        panic!(
            "ThreadX C action E2E failed: accepted={}, completed={}",
            goal_accepted, completed
        );
    }
}
