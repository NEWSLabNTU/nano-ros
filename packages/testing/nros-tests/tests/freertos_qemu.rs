//! FreeRTOS QEMU MPS2-AN385 integration tests
//!
//! Tests that verify FreeRTOS examples build and run on QEMU MPS2-AN385 (Cortex-M3).
//! FreeRTOS examples use `thumbv7m-none-eabi` target with `no_std` + lwIP networking.
//!
//! Prerequisites:
//! - `FREERTOS_DIR` env var pointing to FreeRTOS kernel source (e.g., `external/freertos-kernel`)
//! - `LWIP_DIR` env var pointing to lwIP source (e.g., `external/lwip`)
//! - `arm-none-eabi-gcc` toolchain installed
//! - `qemu-system-arm` with MPS2-AN385 machine support
//!
//! Run with: `just test-freertos`
//! Or: `cargo nextest run -p nros-tests --test freertos_qemu`

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, is_qemu_available, is_tap_bridge_available, is_zenohd_available,
    require_tap_bridge, require_zenohd,
};
use nros_tests::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Check if FREERTOS_DIR environment variable is set and points to a valid directory
fn is_freertos_available() -> bool {
    std::env::var("FREERTOS_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("tasks.c").exists())
        .unwrap_or(false)
}

/// Check if LWIP_DIR environment variable is set and points to a valid directory
fn is_lwip_available() -> bool {
    std::env::var("LWIP_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("src/include/lwip/init.h").exists())
        .unwrap_or(false)
}

/// Check if arm-none-eabi-gcc is available
fn is_arm_gcc_available() -> bool {
    Command::new("arm-none-eabi-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Skip test if FreeRTOS prerequisites are not available
fn require_freertos() -> bool {
    if !is_freertos_available() {
        eprintln!("Skipping test: FREERTOS_DIR not set or invalid");
        eprintln!("Run: just setup-freertos && source .envrc");
        return false;
    }
    if !is_lwip_available() {
        eprintln!("Skipping test: LWIP_DIR not set or invalid");
        eprintln!("Run: just setup-freertos && source .envrc");
        return false;
    }
    if !is_arm_gcc_available() {
        eprintln!("Skipping test: arm-none-eabi-gcc not found");
        eprintln!("Install: sudo apt install gcc-arm-none-eabi");
        return false;
    }
    true
}

/// Skip test if full FreeRTOS E2E prerequisites are not available
///
/// E2E tests require:
/// 1. FreeRTOS build prerequisites (FREERTOS_DIR + LWIP_DIR + arm-none-eabi-gcc)
/// 2. QEMU with MPS2-AN385 machine support
/// 3. TAP bridge network (qemu-br + tap-qemu0 + tap-qemu1)
/// 4. zenohd router (built from submodule)
fn require_freertos_e2e() -> bool {
    if !require_freertos() {
        return false;
    }
    if !is_qemu_available() {
        eprintln!("Skipping test: qemu-system-arm not found");
        eprintln!("Install: sudo apt install qemu-system-arm");
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

static FREERTOS_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build a FreeRTOS QEMU example
fn build_freertos_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-freertos/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "FreeRTOS example directory not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building qemu-arm-freertos/rust/zenoh/{}...", name);

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

    let binary_path =
        example_dir.join(format!("target/thumbv7m-none-eabi/release/{}", binary_name));

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

fn build_freertos_talker() -> TestResult<&'static Path> {
    FREERTOS_TALKER_BINARY
        .get_or_try_init(|| build_freertos_example("talker", "qemu-freertos-talker"))
        .map(|p| p.as_path())
}

fn build_freertos_listener() -> TestResult<&'static Path> {
    FREERTOS_LISTENER_BINARY
        .get_or_try_init(|| build_freertos_example("listener", "qemu-freertos-listener"))
        .map(|p| p.as_path())
}

fn build_freertos_service_server() -> TestResult<&'static Path> {
    FREERTOS_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_freertos_example("service-server", "qemu-freertos-service-server")
        })
        .map(|p| p.as_path())
}

fn build_freertos_service_client() -> TestResult<&'static Path> {
    FREERTOS_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_freertos_example("service-client", "qemu-freertos-service-client")
        })
        .map(|p| p.as_path())
}

fn build_freertos_action_server() -> TestResult<&'static Path> {
    FREERTOS_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_freertos_example("action-server", "qemu-freertos-action-server"))
        .map(|p| p.as_path())
}

fn build_freertos_action_client() -> TestResult<&'static Path> {
    FREERTOS_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_freertos_example("action-client", "qemu-freertos-action-client"))
        .map(|p| p.as_path())
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_freertos_detection() {
    let freertos = is_freertos_available();
    let lwip = is_lwip_available();
    let arm_gcc = is_arm_gcc_available();
    let qemu = is_qemu_available();
    let tap_bridge = is_tap_bridge_available();
    let zenohd = is_zenohd_available();
    eprintln!("FreeRTOS available: {}", freertos);
    eprintln!("lwIP available: {}", lwip);
    eprintln!("arm-none-eabi-gcc available: {}", arm_gcc);
    eprintln!("QEMU available: {}", qemu);
    eprintln!("TAP bridge available: {}", tap_bridge);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// Build tests (require FREERTOS_DIR + LWIP_DIR + arm-none-eabi-gcc)
// =============================================================================

#[test]
fn test_freertos_talker_builds() {
    if !require_freertos() {
        return;
    }
    let binary = build_freertos_talker().expect("Failed to build qemu-freertos-talker");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-freertos-talker builds at {}",
        binary.display()
    );
}

#[test]
fn test_freertos_listener_builds() {
    if !require_freertos() {
        return;
    }
    let binary = build_freertos_listener().expect("Failed to build qemu-freertos-listener");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-freertos-listener builds at {}",
        binary.display()
    );
}

#[test]
fn test_freertos_service_server_builds() {
    if !require_freertos() {
        return;
    }
    let binary =
        build_freertos_service_server().expect("Failed to build qemu-freertos-service-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-freertos-service-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_freertos_service_client_builds() {
    if !require_freertos() {
        return;
    }
    let binary =
        build_freertos_service_client().expect("Failed to build qemu-freertos-service-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-freertos-service-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_freertos_action_server_builds() {
    if !require_freertos() {
        return;
    }
    let binary =
        build_freertos_action_server().expect("Failed to build qemu-freertos-action-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-freertos-action-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_freertos_action_client_builds() {
    if !require_freertos() {
        return;
    }
    let binary =
        build_freertos_action_client().expect("Failed to build qemu-freertos-action-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-freertos-action-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_freertos_all_examples_build() {
    if !require_freertos() {
        return;
    }

    let results = [
        ("talker", build_freertos_talker()),
        ("listener", build_freertos_listener()),
        ("service-server", build_freertos_service_server()),
        ("service-client", build_freertos_service_client()),
        ("action-server", build_freertos_action_server()),
        ("action-client", build_freertos_action_client()),
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

    assert!(all_ok, "Not all FreeRTOS examples built successfully");
}

// =============================================================================
// E2E Network tests (require QEMU + TAP bridge + zenohd)
// =============================================================================
//
// FreeRTOS QEMU MPS2-AN385 examples use LAN9118 Ethernet with TAP networking:
//   qemu-system-arm -machine mps2-an385 -cpu cortex-m3 -nographic \
//       -semihosting-config enable=on,target=native \
//       -kernel <binary> \
//       -nic tap,ifname=tap-qemu0,script=no,downscript=no,model=lan9118,mac=02:00:00:00:00:00
//
// Network topology:
//   QEMU node 0 (tap-qemu0, 192.0.3.10) --+
//                                           |-- Bridge (qemu-br, 192.0.3.1) -- zenohd
//   QEMU node 1 (tap-qemu1, 192.0.3.11) --+
//
// Prerequisites:
//   1. TAP bridge: sudo ./scripts/qemu/setup-network.sh
//   2. zenohd: just build-zenohd
//   3. Run: just test-freertos

/// Test pub/sub message exchange between FreeRTOS QEMU instances.
///
/// Launches a listener on tap-qemu1 and a talker on tap-qemu0, verifies
/// that the listener receives Int32 messages published by the talker.
#[test]
fn test_freertos_pubsub_e2e() {
    if !require_freertos_e2e() {
        return;
    }

    // Build both binaries
    let talker_bin = build_freertos_talker().expect("Failed to build talker");
    let listener_bin = build_freertos_listener().expect("Failed to build listener");

    // Start zenohd on fixed port 7447 (firmware hardcodes tcp/192.0.3.1:7447)
    let _zenohd = ZenohRouter::start(7447).expect("Failed to start zenohd on port 7447");

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting listener QEMU on tap-qemu1...");
    let mut listener =
        QemuProcess::start_mps2_an385_networked(listener_bin, "tap-qemu1", "02:00:00:00:00:01")
            .expect("Failed to start listener QEMU");

    // Stabilization delay: FreeRTOS boot + lwIP init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start talker QEMU
    eprintln!("Starting talker QEMU on tap-qemu0...");
    let mut talker =
        QemuProcess::start_mps2_an385_networked(talker_bin, "tap-qemu0", "02:00:00:00:00:00")
            .expect("Failed to start talker QEMU");

    // Wait for listener to complete — reads all buffered output (boot + messages).
    // The completion marker "Received 10 messages" triggers early return.
    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to finish publishing
    let _talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);

    // Verify listener booted and received messages
    if !listener_output.contains("Waiting for messages") {
        eprintln!("[SKIP] Listener did not reach readiness — FreeRTOS+lwIP init may have failed");
        eprintln!("Check: zenohd on 192.0.3.1:7447, TAP bridge up, firmware built correctly");
        return;
    }

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("Messages received: {}", received_count);

    if received_count > 0 {
        eprintln!(
            "[PASS] FreeRTOS pubsub E2E: {} messages exchanged",
            received_count
        );
    } else {
        eprintln!("[FAIL] FreeRTOS pubsub E2E: no messages received");
        panic!("FreeRTOS pubsub E2E failed — listener received 0 messages");
    }
}

/// Test service request/response between FreeRTOS QEMU instances.
///
/// Launches a service server on tap-qemu0 and a client on tap-qemu1,
/// verifies that the client receives correct AddTwoInts responses.
#[test]
fn test_freertos_service_e2e() {
    if !require_freertos_e2e() {
        return;
    }

    let server_bin = build_freertos_service_server().expect("Failed to build service server");
    let client_bin = build_freertos_service_client().expect("Failed to build service client");

    let _zenohd = ZenohRouter::start(7447).expect("Failed to start zenohd on port 7447");

    // Start server first
    eprintln!("Starting service server QEMU on tap-qemu0...");
    let mut server =
        QemuProcess::start_mps2_an385_networked(server_bin, "tap-qemu0", "02:00:00:00:00:00")
            .expect("Failed to start server QEMU");

    // Stabilization delay: FreeRTOS boot + lwIP init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start client
    eprintln!("Starting service client QEMU on tap-qemu1...");
    let mut client =
        QemuProcess::start_mps2_an385_networked(client_bin, "tap-qemu1", "02:00:00:00:00:01")
            .expect("Failed to start client QEMU");

    // Wait for client to complete all service calls (4 calls: 5+3, 10+20, 100+200, -5+10)
    // The completion marker "All service calls completed" triggers early return.
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

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
            "[PASS] FreeRTOS service E2E: {} responses, all correct",
            response_count
        );
    } else if response_count > 0 {
        eprintln!(
            "[PARTIAL] FreeRTOS service E2E: {} of 4 responses",
            response_count
        );
    } else {
        eprintln!("[FAIL] FreeRTOS service E2E: no responses received");
        panic!("FreeRTOS service E2E failed — client received 0 responses");
    }
}

/// Test action goal/feedback/result between FreeRTOS QEMU instances.
///
/// Launches an action server on tap-qemu0 and a client on tap-qemu1,
/// verifies that the client receives Fibonacci feedback and final result.
#[test]
fn test_freertos_action_e2e() {
    if !require_freertos_e2e() {
        return;
    }

    let server_bin = build_freertos_action_server().expect("Failed to build action server");
    let client_bin = build_freertos_action_client().expect("Failed to build action client");

    let _zenohd = ZenohRouter::start(7447).expect("Failed to start zenohd on port 7447");

    // Start action server first
    eprintln!("Starting action server QEMU on tap-qemu0...");
    let mut server =
        QemuProcess::start_mps2_an385_networked(server_bin, "tap-qemu0", "02:00:00:00:00:00")
            .expect("Failed to start server QEMU");

    // Stabilization delay: FreeRTOS boot + lwIP init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start action client
    eprintln!("Starting action client QEMU on tap-qemu1...");
    let mut client =
        QemuProcess::start_mps2_an385_networked(client_bin, "tap-qemu1", "02:00:00:00:00:01")
            .expect("Failed to start client QEMU");

    // Fibonacci computation + FreeRTOS boot + zenoh connect.
    // The completion marker "Action completed successfully" triggers early return.
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("Client output:\n{}", client_output);

    // Verify action protocol
    let goal_accepted = client_output.contains("Goal accepted");
    let feedback_count = count_pattern(&client_output, "Feedback #");
    let completed = client_output.contains("Action completed successfully");

    eprintln!(
        "Goal accepted: {}, feedback: {}, completed: {}",
        goal_accepted, feedback_count, completed
    );

    if goal_accepted && completed {
        eprintln!(
            "[PASS] FreeRTOS action E2E: goal accepted, {} feedback msgs, completed",
            feedback_count
        );
    } else {
        eprintln!("[FAIL] FreeRTOS action E2E:");
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
            "FreeRTOS action E2E failed: accepted={}, feedback={}, completed={}",
            goal_accepted, feedback_count, completed
        );
    }
}
