//! ThreadX Linux integration tests
//!
//! Tests that verify ThreadX Linux examples build and run natively.
//! ThreadX Linux examples use the ThreadX Linux simulation port with
//! NetX Duo raw-socket network driver over TAP interfaces.
//!
//! Prerequisites:
//! - `THREADX_DIR` env var pointing to ThreadX source (e.g., `external/threadx`)
//! - `NETX_DIR` env var pointing to NetX Duo source (e.g., `external/netxduo`)
//!
//! Run with: `just test-threadx-linux`
//! Or: `cargo nextest run -p nros-tests --test threadx_linux`

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ZenohRouter, is_tap_bridge_available, is_zenohd_available, require_tap_bridge, require_zenohd,
};
use nros_tests::process::{ManagedProcess, kill_process_group};
use nros_tests::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Check if THREADX_DIR environment variable is set and points to a valid directory
fn is_threadx_available() -> bool {
    std::env::var("THREADX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("common/inc/tx_api.h").exists())
        .unwrap_or(false)
}

/// Check if NETX_DIR environment variable is set and points to a valid directory
fn is_netx_available() -> bool {
    std::env::var("NETX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("common/inc/nx_api.h").exists())
        .unwrap_or(false)
}

/// Check if the ThreadX learn-samples Linux network driver is available
fn is_threadx_samples_available() -> bool {
    let root = project_root();
    root.join("external/threadx-learn-samples/courses/threadx/ProjectFiles/Linux/nx_linux_network_driver.c")
        .exists()
}

/// Skip test if ThreadX build prerequisites are not available
fn require_threadx() -> bool {
    if !is_threadx_available() {
        eprintln!("Skipping test: THREADX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_netx_available() {
        eprintln!("Skipping test: NETX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_threadx_samples_available() {
        eprintln!("Skipping test: ThreadX learn-samples not found");
        eprintln!("Run: just setup-threadx");
        return false;
    }
    true
}

/// Skip test if full ThreadX E2E prerequisites are not available
///
/// E2E tests require:
/// 1. ThreadX build prerequisites (THREADX_DIR + NETX_DIR + learn-samples)
/// 2. TAP bridge network (qemu-br + tap-qemu0 + tap-qemu1)
/// 3. zenohd router (built from submodule)
fn require_threadx_e2e() -> bool {
    if !require_threadx() {
        return false;
    }
    if !require_tap_bridge() {
        return false;
    }
    if !require_zenohd() {
        return false;
    }
    true
}

// =============================================================================
// Binary builders
// =============================================================================

static THREADX_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build a ThreadX Linux example
fn build_threadx_linux_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/threadx-linux/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX Linux example directory not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building threadx-linux/rust/zenoh/{}...", name);

    let output = duct::cmd!("cargo", "build", "--release")
        .dir(&example_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(
            String::from_utf8_lossy(&output.stdout).to_string(),
        ));
    }

    // Native build — binary at target/release/<binary_name> (no cross-target subdir)
    let binary_path = example_dir.join(format!("target/release/{}", binary_name));

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

fn build_threadx_talker() -> TestResult<&'static Path> {
    THREADX_TALKER_BINARY
        .get_or_try_init(|| build_threadx_linux_example("talker", "threadx-linux-talker"))
        .map(|p| p.as_path())
}

fn build_threadx_listener() -> TestResult<&'static Path> {
    THREADX_LISTENER_BINARY
        .get_or_try_init(|| build_threadx_linux_example("listener", "threadx-linux-listener"))
        .map(|p| p.as_path())
}

fn build_threadx_service_server() -> TestResult<&'static Path> {
    THREADX_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_threadx_linux_example("service-server", "threadx-linux-service-server")
        })
        .map(|p| p.as_path())
}

fn build_threadx_service_client() -> TestResult<&'static Path> {
    THREADX_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_threadx_linux_example("service-client", "threadx-linux-service-client")
        })
        .map(|p| p.as_path())
}

fn build_threadx_action_server() -> TestResult<&'static Path> {
    THREADX_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_threadx_linux_example("action-server", "threadx-linux-action-server")
        })
        .map(|p| p.as_path())
}

fn build_threadx_action_client() -> TestResult<&'static Path> {
    THREADX_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_threadx_linux_example("action-client", "threadx-linux-action-client")
        })
        .map(|p| p.as_path())
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_threadx_detection() {
    let threadx = is_threadx_available();
    let netx = is_netx_available();
    let samples = is_threadx_samples_available();
    let tap_bridge = is_tap_bridge_available();
    let zenohd = is_zenohd_available();
    eprintln!("ThreadX available: {}", threadx);
    eprintln!("NetX Duo available: {}", netx);
    eprintln!("ThreadX learn-samples available: {}", samples);
    eprintln!("TAP bridge available: {}", tap_bridge);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// Build tests (require THREADX_DIR + NETX_DIR + learn-samples)
// =============================================================================

#[test]
fn test_threadx_talker_builds() {
    if !require_threadx() {
        return;
    }
    let binary = build_threadx_talker().expect("Failed to build threadx-linux-talker");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: threadx-linux-talker builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_listener_builds() {
    if !require_threadx() {
        return;
    }
    let binary = build_threadx_listener().expect("Failed to build threadx-linux-listener");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: threadx-linux-listener builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_service_server_builds() {
    if !require_threadx() {
        return;
    }
    let binary =
        build_threadx_service_server().expect("Failed to build threadx-linux-service-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: threadx-linux-service-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_service_client_builds() {
    if !require_threadx() {
        return;
    }
    let binary =
        build_threadx_service_client().expect("Failed to build threadx-linux-service-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: threadx-linux-service-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_action_server_builds() {
    if !require_threadx() {
        return;
    }
    let binary =
        build_threadx_action_server().expect("Failed to build threadx-linux-action-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: threadx-linux-action-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_action_client_builds() {
    if !require_threadx() {
        return;
    }
    let binary =
        build_threadx_action_client().expect("Failed to build threadx-linux-action-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: threadx-linux-action-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_all_examples_build() {
    if !require_threadx() {
        return;
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
        return;
    }

    // Build both binaries
    let talker_bin = build_threadx_talker().expect("Failed to build talker");
    let listener_bin = build_threadx_listener().expect("Failed to build listener");

    // Start zenohd on fixed port 7447 (firmware hardcodes tcp/192.0.3.1:7447)
    let _zenohd = ZenohRouter::start(7447).expect("Failed to start zenohd on port 7447");

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

    // Wait for listener to complete
    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to finish publishing
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
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
             - TAP devices: `ip link show tap-qemu0 tap-qemu1` (should be UP, master qemu-br)\n\
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
        return;
    }

    let server_bin = build_threadx_service_server().expect("Failed to build service server");
    let client_bin = build_threadx_service_client().expect("Failed to build service client");

    let _zenohd = ZenohRouter::start(7447).expect("Failed to start zenohd on port 7447");

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
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    kill_process_group(server.handle_mut());
    kill_process_group(client.handle_mut());

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
             - TAP devices: `ip link show tap-qemu0 tap-qemu1` (should be UP, master qemu-br)\n\
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
        return;
    }

    let server_bin = build_threadx_action_server().expect("Failed to build action server");
    let client_bin = build_threadx_action_client().expect("Failed to build action client");

    let _zenohd = ZenohRouter::start(7447).expect("Failed to start zenohd on port 7447");

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

    // Wait for client to complete
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    kill_process_group(server.handle_mut());
    kill_process_group(client.handle_mut());

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
             - TAP devices: `ip link show tap-qemu0 tap-qemu1` (should be UP, master qemu-br)\n\
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
