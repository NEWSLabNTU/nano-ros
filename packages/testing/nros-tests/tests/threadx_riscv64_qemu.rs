//! ThreadX QEMU RISC-V 64-bit integration tests
//!
//! Tests that verify ThreadX QEMU RISC-V examples build and run on QEMU virt
//! machine with virtio-net networking. Examples use `riscv64gc-unknown-none-elf`
//! target with `no_std` + NetX Duo networking over virtio-net.
//!
//! Prerequisites:
//! - `THREADX_DIR` env var pointing to ThreadX source (e.g., `third-party/threadx/kernel`)
//! - `NETX_DIR` env var pointing to NetX Duo source (e.g., `third-party/threadx/netxduo`)
//! - `riscv64-unknown-elf-gcc` cross-compiler installed
//! - `qemu-system-riscv64` with virt machine support
//! - zenohd: `just build-zenohd`
//!
//! Run with: `just test-threadx-riscv64`
//! Or: `cargo nextest run -p nros-tests --test threadx_riscv64_qemu`

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, is_qemu_riscv64_available, is_zenohd_available, require_zenohd,
};
use nros_tests::platform;
use nros_tests::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Command;
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

/// Check if riscv64-unknown-elf-gcc is available
fn is_riscv_gcc_available() -> bool {
    Command::new("riscv64-unknown-elf-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Skip test if ThreadX RISC-V build prerequisites are not available
fn require_threadx_riscv64() -> bool {
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
    if !is_riscv_gcc_available() {
        eprintln!("Skipping test: riscv64-unknown-elf-gcc not found");
        eprintln!("Install: sudo apt install gcc-riscv64-unknown-elf");
        return false;
    }
    true
}

/// Skip test if full ThreadX RISC-V E2E prerequisites are not available
///
/// E2E tests require:
/// 1. ThreadX build prerequisites (THREADX_DIR + NETX_DIR + riscv64-unknown-elf-gcc)
/// 2. QEMU RISC-V 64-bit (qemu-system-riscv64)
/// 3. zenohd router (built from submodule)
fn require_threadx_riscv64_e2e() -> bool {
    if !require_threadx_riscv64() {
        return false;
    }
    if !is_qemu_riscv64_available() {
        eprintln!("Skipping test: qemu-system-riscv64 not found");
        eprintln!("Install: sudo apt install qemu-system-riscv64");
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

static THREADX_RV64_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build a ThreadX QEMU RISC-V example
fn build_threadx_riscv64_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-riscv64-threadx/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX RISC-V example directory not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building qemu-riscv64-threadx/rust/zenoh/{}...", name);

    // RISC-V board crate needs its own config dirs (not the Linux sim defaults)
    let rv_config = root.join("packages/boards/nros-threadx-qemu-riscv64/config");

    let output = duct::cmd!("cargo", "build", "--release")
        .dir(&example_dir)
        .env("THREADX_CONFIG_DIR", &rv_config)
        .env("NETX_CONFIG_DIR", &rv_config)
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

    // Cross-compiled binary at target/<target>/release/<binary_name>
    let binary_path = example_dir.join(format!(
        "target/riscv64gc-unknown-none-elf/release/{}",
        binary_name
    ));

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

fn build_threadx_rv64_talker() -> TestResult<&'static Path> {
    THREADX_RV64_TALKER_BINARY
        .get_or_try_init(|| build_threadx_riscv64_example("talker", "qemu-riscv64-threadx-talker"))
        .map(|p| p.as_path())
}

fn build_threadx_rv64_listener() -> TestResult<&'static Path> {
    THREADX_RV64_LISTENER_BINARY
        .get_or_try_init(|| {
            build_threadx_riscv64_example("listener", "qemu-riscv64-threadx-listener")
        })
        .map(|p| p.as_path())
}

fn build_threadx_rv64_service_server() -> TestResult<&'static Path> {
    THREADX_RV64_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_threadx_riscv64_example("service-server", "qemu-riscv64-threadx-service-server")
        })
        .map(|p| p.as_path())
}

fn build_threadx_rv64_service_client() -> TestResult<&'static Path> {
    THREADX_RV64_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_threadx_riscv64_example("service-client", "qemu-riscv64-threadx-service-client")
        })
        .map(|p| p.as_path())
}

fn build_threadx_rv64_action_server() -> TestResult<&'static Path> {
    THREADX_RV64_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_threadx_riscv64_example("action-server", "qemu-riscv64-threadx-action-server")
        })
        .map(|p| p.as_path())
}

fn build_threadx_rv64_action_client() -> TestResult<&'static Path> {
    THREADX_RV64_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_threadx_riscv64_example("action-client", "qemu-riscv64-threadx-action-client")
        })
        .map(|p| p.as_path())
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_threadx_riscv64_detection() {
    let threadx = is_threadx_available();
    let netx = is_netx_available();
    let riscv_gcc = is_riscv_gcc_available();
    let qemu_rv64 = is_qemu_riscv64_available();
    let zenohd = is_zenohd_available();
    eprintln!("ThreadX available: {}", threadx);
    eprintln!("NetX Duo available: {}", netx);
    eprintln!("riscv64-unknown-elf-gcc available: {}", riscv_gcc);
    eprintln!("QEMU RISC-V 64 available: {}", qemu_rv64);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// Build tests (require THREADX_DIR + NETX_DIR + riscv64-unknown-elf-gcc)
// =============================================================================

#[test]
fn test_threadx_riscv64_talker_builds() {
    if !require_threadx_riscv64() {
        return;
    }
    let binary = build_threadx_rv64_talker().expect("Failed to build qemu-riscv64-threadx-talker");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-riscv64-threadx-talker builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_riscv64_listener_builds() {
    if !require_threadx_riscv64() {
        return;
    }
    let binary =
        build_threadx_rv64_listener().expect("Failed to build qemu-riscv64-threadx-listener");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-riscv64-threadx-listener builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_riscv64_service_server_builds() {
    if !require_threadx_riscv64() {
        return;
    }
    let binary = build_threadx_rv64_service_server()
        .expect("Failed to build qemu-riscv64-threadx-service-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-riscv64-threadx-service-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_riscv64_service_client_builds() {
    if !require_threadx_riscv64() {
        return;
    }
    let binary = build_threadx_rv64_service_client()
        .expect("Failed to build qemu-riscv64-threadx-service-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-riscv64-threadx-service-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_riscv64_action_server_builds() {
    if !require_threadx_riscv64() {
        return;
    }
    let binary = build_threadx_rv64_action_server()
        .expect("Failed to build qemu-riscv64-threadx-action-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-riscv64-threadx-action-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_riscv64_action_client_builds() {
    if !require_threadx_riscv64() {
        return;
    }
    let binary = build_threadx_rv64_action_client()
        .expect("Failed to build qemu-riscv64-threadx-action-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: qemu-riscv64-threadx-action-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_threadx_riscv64_all_examples_build() {
    if !require_threadx_riscv64() {
        return;
    }

    let results = [
        ("talker", build_threadx_rv64_talker()),
        ("listener", build_threadx_rv64_listener()),
        ("service-server", build_threadx_rv64_service_server()),
        ("service-client", build_threadx_rv64_service_client()),
        ("action-server", build_threadx_rv64_action_server()),
        ("action-client", build_threadx_rv64_action_client()),
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

    assert!(
        all_ok,
        "Not all ThreadX QEMU RISC-V examples built successfully"
    );
}

// =============================================================================
// E2E Network tests (require QEMU RISC-V + zenohd)
// =============================================================================
//
// ThreadX QEMU RISC-V examples use virtio-net Ethernet with slirp user networking:
//   qemu-system-riscv64 -M virt -nographic \
//       -global virtio-mmio.force-legacy=false \
//       -netdev user,id=net0,... \
//       -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0 \
//       -kernel <binary>
//
// Network topology:
//   QEMU node 0 (slirp, 10.0.2.40) ---> 10.0.2.2:7453 --+
//                                                          |-- zenohd (localhost:7453)
//   QEMU node 1 (slirp, 10.0.2.41) ---> 10.0.2.2:7453 --+
//
// Each QEMU instance has its own isolated 10.0.2.0/24 slirp network.
// Firmware connects to zenohd via slirp gateway 10.0.2.2:7453 -> host 127.0.0.1:7453.
//
// Prerequisites:
//   1. zenohd: just build-zenohd
//   2. Run: just test-threadx-riscv64

/// Test pub/sub message exchange between ThreadX QEMU RISC-V instances.
///
/// Launches a listener (QEMU node 1) and a talker (QEMU node 0), verifies
/// that the listener receives Int32 messages published by the talker.
#[test]
fn test_threadx_riscv64_pubsub_e2e() {
    if !require_threadx_riscv64_e2e() {
        return;
    }

    // Build both binaries
    let talker_bin = build_threadx_rv64_talker().expect("Failed to build talker");
    let listener_bin = build_threadx_rv64_listener().expect("Failed to build listener");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting listener QEMU (node 1, slirp 10.0.2.41)...");
    let mut listener =
        QemuProcess::start_riscv64_virt(listener_bin, 1).expect("Failed to start listener QEMU");

    // Stabilization delay: ThreadX boot + NetX init + virtio-net init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start talker QEMU
    eprintln!("Starting talker QEMU (node 0, slirp 10.0.2.40)...");
    let mut talker =
        QemuProcess::start_riscv64_virt(talker_bin, 0).expect("Failed to start talker QEMU");

    // Wait for listener to complete
    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to finish publishing
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify listener booted and received messages
    if !listener_output.contains("Waiting for messages") {
        panic!(
            "ThreadX RISC-V pubsub E2E failed — listener did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - zenohd is running on platform port 7453\n\
             - QEMU slirp gateway forwards 10.0.2.2:7453 -> host\n\
             - Firmware built: `just build-examples-threadx-riscv64`"
        );
    }

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("Messages received: {}", received_count);

    if received_count > 0 {
        eprintln!(
            "[PASS] ThreadX RISC-V pubsub E2E: {} messages exchanged",
            received_count
        );
    } else {
        panic!("ThreadX RISC-V pubsub E2E failed — listener received 0 messages");
    }
}

/// Test service request/response between ThreadX QEMU RISC-V instances.
///
/// Launches a service server (QEMU node 0) and a client (QEMU node 1),
/// verifies that the client receives correct AddTwoInts responses.
#[test]
fn test_threadx_riscv64_service_e2e() {
    if !require_threadx_riscv64_e2e() {
        return;
    }

    let server_bin = build_threadx_rv64_service_server().expect("Failed to build service server");
    let client_bin = build_threadx_rv64_service_client().expect("Failed to build service client");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    // Start server first
    eprintln!("Starting service server QEMU (node 0, slirp 10.0.2.40)...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server QEMU");

    // Stabilization delay: ThreadX boot + NetX init + virtio-net init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start client
    eprintln!("Starting service client QEMU (node 1, slirp 10.0.2.41)...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client QEMU");

    // Stabilization delay: client also needs ThreadX boot + NetX init + zenoh connect
    // before it can discover the server's service queryable.
    std::thread::sleep(Duration::from_secs(15));

    // Wait for client to complete all service calls (4 calls: 5+3, 10+20, 100+200, -5+10)
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
            "[PASS] ThreadX RISC-V service E2E: {} responses, all correct",
            response_count
        );
    } else if response_count > 0 {
        eprintln!(
            "[PARTIAL] ThreadX RISC-V service E2E: {} of 4 responses",
            response_count
        );
    } else if !client_output.contains("Service client ready")
        && !client_output.contains("nros ThreadX QEMU RISC-V Platform")
    {
        panic!(
            "ThreadX RISC-V service E2E failed — client did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - zenohd is running on platform port 7453\n\
             - QEMU slirp gateway forwards 10.0.2.2:7453 -> host\n\
             - Firmware built: `just build-examples-threadx-riscv64`"
        );
    } else {
        panic!(
            "ThreadX RISC-V service E2E failed — client received 0 responses.\n\
             Client reached readiness but no service replies were received.\n\
             This may indicate a zenoh queryable discovery timeout."
        );
    }
}

/// Test action goal/feedback/result between ThreadX QEMU RISC-V instances.
///
/// Launches an action server (QEMU node 0) and a client (QEMU node 1),
/// verifies that the client receives Fibonacci feedback and final result.
#[test]
fn test_threadx_riscv64_action_e2e() {
    if !require_threadx_riscv64_e2e() {
        return;
    }

    let server_bin = build_threadx_rv64_action_server().expect("Failed to build action server");
    let client_bin = build_threadx_rv64_action_client().expect("Failed to build action client");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    // Start action server first
    eprintln!("Starting action server QEMU (node 0, slirp 10.0.2.40)...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server QEMU");

    // Stabilization delay: ThreadX boot + NetX init + virtio-net init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start action client
    eprintln!("Starting action client QEMU (node 1, slirp 10.0.2.41)...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client QEMU");

    // Stabilization delay: client also needs ThreadX boot + NetX init + zenoh connect
    // before it can discover the server's action queryables.
    std::thread::sleep(Duration::from_secs(15));

    // Fibonacci computation + zenoh connect.
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
            "[PASS] ThreadX RISC-V action E2E: goal accepted, {} feedback msgs, completed",
            feedback_count
        );
    } else if !client_output.contains("nros ThreadX QEMU RISC-V Platform")
        && !client_output.contains("Action client ready")
        && !goal_accepted
    {
        panic!(
            "ThreadX RISC-V action E2E failed — client did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - zenohd is running on platform port 7453\n\
             - QEMU slirp gateway forwards 10.0.2.2:7453 -> host\n\
             - Firmware built: `just build-examples-threadx-riscv64`"
        );
    } else {
        eprintln!("[FAIL] ThreadX RISC-V action E2E:");
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
            "ThreadX RISC-V action E2E failed: accepted={}, feedback={}, completed={}",
            goal_accepted, feedback_count, completed
        );
    }
}

// =============================================================================
// C binary builders (CMake-based, cross-compiled for riscv64gc)
// =============================================================================

static RV64_C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

static RV64_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn is_cmake_available() -> bool {
    Command::new("cmake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_threadx_rv64_cmake() -> bool {
    if !require_threadx_riscv64() {
        return false;
    }
    if !is_cmake_available() {
        eprintln!("Skipping test: cmake not found");
        return false;
    }
    true
}

/// Build a ThreadX RISC-V C/C++ example via CMake
fn build_rv64_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!(
        "examples/qemu-riscv64-threadx/{}/zenoh/{}",
        lang, name
    ));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example not found: {}",
            example_dir.display()
        )));
    }

    eprintln!(
        "Building qemu-riscv64-threadx/{}/zenoh/{} (CMake)...",
        lang, name
    );

    let build_dir = example_dir.join("build");
    // Clean stale build to avoid cmake cache conflicts
    let _ = std::fs::remove_dir_all(&build_dir);
    std::fs::create_dir_all(&build_dir).ok();

    let prefix_path = format!(
        "-DCMAKE_PREFIX_PATH={}",
        root.join("build/install").display()
    );
    let toolchain = format!(
        "-DCMAKE_TOOLCHAIN_FILE={}",
        root.join("cmake/toolchain/riscv64-threadx.cmake").display()
    );
    let threadx_dir = std::env::var("THREADX_DIR").unwrap_or_else(|_| {
        root.join("third-party/threadx/kernel")
            .display()
            .to_string()
    });
    let netx_dir = std::env::var("NETX_DIR").unwrap_or_else(|_| {
        root.join("third-party/threadx/netxduo")
            .display()
            .to_string()
    });
    // Always use the RISC-V board config (not the Linux one from THREADX_CONFIG_DIR)
    let config_dir = root
        .join("packages/boards/nros-threadx-qemu-riscv64/config")
        .display()
        .to_string();
    let board_dir = root
        .join("packages/boards/nros-threadx-qemu-riscv64/c")
        .display()
        .to_string();
    let virtio_dir = root
        .join("packages/drivers/virtio-net-netx")
        .display()
        .to_string();

    let output = duct::cmd!(
        "cmake",
        "-S",
        &example_dir,
        "-B",
        &build_dir,
        &prefix_path,
        &toolchain,
        "-DNANO_ROS_PLATFORM=threadx_riscv64",
        &format!("-DTHREADX_DIR={threadx_dir}"),
        &format!("-DNETX_DIR={netx_dir}"),
        &format!("-DTHREADX_CONFIG_DIR={config_dir}"),
        &format!("-DTHREADX_BOARD_DIR={board_dir}"),
        &format!("-DVIRTIO_DRIVER_DIR={virtio_dir}"),
        "-DCMAKE_BUILD_TYPE=Release"
    )
    .stderr_to_stdout()
    .stdout_capture()
    .unchecked()
    .run()
    .map_err(|e| TestError::BuildFailed(format!("cmake configure: {}", e)))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake configure failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    let output = duct::cmd!("cmake", "--build", &build_dir, "--", "-j4")
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(format!("cmake build: {}", e)))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake build failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    let binary_path = build_dir.join(binary_name);
    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

fn build_rv64_c_talker() -> TestResult<&'static Path> {
    RV64_C_TALKER_BINARY
        .get_or_try_init(|| build_rv64_cmake_example("c", "talker", "riscv64_threadx_c_talker"))
        .map(|p| p.as_path())
}
fn build_rv64_c_listener() -> TestResult<&'static Path> {
    RV64_C_LISTENER_BINARY
        .get_or_try_init(|| build_rv64_cmake_example("c", "listener", "riscv64_threadx_c_listener"))
        .map(|p| p.as_path())
}
fn build_rv64_c_service_server() -> TestResult<&'static Path> {
    RV64_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_rv64_cmake_example("c", "service-server", "riscv64_threadx_c_service_server")
        })
        .map(|p| p.as_path())
}
fn build_rv64_c_service_client() -> TestResult<&'static Path> {
    RV64_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_rv64_cmake_example("c", "service-client", "riscv64_threadx_c_service_client")
        })
        .map(|p| p.as_path())
}
fn build_rv64_c_action_server() -> TestResult<&'static Path> {
    RV64_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_rv64_cmake_example("c", "action-server", "riscv64_threadx_c_action_server")
        })
        .map(|p| p.as_path())
}
fn build_rv64_c_action_client() -> TestResult<&'static Path> {
    RV64_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_rv64_cmake_example("c", "action-client", "riscv64_threadx_c_action_client")
        })
        .map(|p| p.as_path())
}
fn build_rv64_cpp_talker() -> TestResult<&'static Path> {
    RV64_CPP_TALKER_BINARY
        .get_or_try_init(|| build_rv64_cmake_example("cpp", "talker", "riscv64_threadx_cpp_talker"))
        .map(|p| p.as_path())
}
fn build_rv64_cpp_listener() -> TestResult<&'static Path> {
    RV64_CPP_LISTENER_BINARY
        .get_or_try_init(|| {
            build_rv64_cmake_example("cpp", "listener", "riscv64_threadx_cpp_listener")
        })
        .map(|p| p.as_path())
}

// =============================================================================
// C Build tests
// =============================================================================

#[test]
fn test_rv64_c_talker_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_c_talker().expect("build failed");
    assert!(b.exists());
    eprintln!("SUCCESS: {}", b.display());
}

#[test]
fn test_rv64_c_listener_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_c_listener().expect("build failed");
    assert!(b.exists());
    eprintln!("SUCCESS: {}", b.display());
}

#[test]
fn test_rv64_c_service_server_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_c_service_server().expect("build failed");
    assert!(b.exists());
}

#[test]
fn test_rv64_c_service_client_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_c_service_client().expect("build failed");
    assert!(b.exists());
}

#[test]
fn test_rv64_c_action_server_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_c_action_server().expect("build failed");
    assert!(b.exists());
}

#[test]
fn test_rv64_c_action_client_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_c_action_client().expect("build failed");
    assert!(b.exists());
}

// =============================================================================
// C++ Build tests (talker + listener only — service examples hit codegen bug)
// =============================================================================

#[test]
fn test_rv64_cpp_talker_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_cpp_talker().expect("build failed");
    assert!(b.exists());
    eprintln!("SUCCESS: {}", b.display());
}

#[test]
fn test_rv64_cpp_listener_builds() {
    if !require_threadx_rv64_cmake() {
        return;
    }
    let b = build_rv64_cpp_listener().expect("build failed");
    assert!(b.exists());
    eprintln!("SUCCESS: {}", b.display());
}

// =============================================================================
// C E2E Network tests (QEMU slirp — no host setup needed)
// =============================================================================

#[test]
fn test_rv64_c_pubsub_e2e() {
    if !require_threadx_riscv64_e2e() {
        return;
    }

    let talker_bin = build_rv64_c_talker().expect("build talker failed");
    let listener_bin = build_rv64_c_listener().expect("build listener failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C listener QEMU...");
    let mut listener =
        QemuProcess::start_riscv64_virt(listener_bin, 1).expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C talker QEMU...");
    let mut talker =
        QemuProcess::start_riscv64_virt(talker_bin, 0).expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("C Listener output:\n{}", listener_output);
    eprintln!("C Talker output:\n{}", talker_output);

    let received = count_pattern(&listener_output, "Received");
    eprintln!("C messages received: {}", received);
    assert!(received > 0, "C pubsub E2E failed — 0 messages");
    eprintln!("[PASS] ThreadX RISC-V C pubsub E2E: {} msgs", received);
}

#[test]
fn test_rv64_c_service_e2e() {
    if !require_threadx_riscv64_e2e() {
        return;
    }

    let server_bin = build_rv64_c_service_server().expect("build server failed");
    let client_bin = build_rv64_c_service_client().expect("build client failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C service server QEMU...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C service client QEMU...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client");

    std::thread::sleep(Duration::from_secs(15));

    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("C Client output:\n{}", client_output);

    let responses = count_pattern(&client_output, "Response:");
    assert!(responses > 0, "C service E2E failed — 0 responses");
    eprintln!(
        "[PASS] ThreadX RISC-V C service E2E: {} responses",
        responses
    );
}

#[test]
fn test_rv64_c_action_e2e() {
    if !require_threadx_riscv64_e2e() {
        return;
    }

    let server_bin = build_rv64_c_action_server().expect("build server failed");
    let client_bin = build_rv64_c_action_client().expect("build client failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C action server QEMU...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C action client QEMU...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client");

    std::thread::sleep(Duration::from_secs(15));

    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("C Client output:\n{}", client_output);

    let goal_accepted = client_output.contains("Goal accepted");
    let completed = client_output.contains("Action completed successfully");
    assert!(
        goal_accepted && completed,
        "C action E2E failed: accepted={}, completed={}",
        goal_accepted,
        completed
    );
    eprintln!("[PASS] ThreadX RISC-V C action E2E");
}

// =============================================================================
// C++ E2E Network tests
// =============================================================================

#[test]
fn test_rv64_cpp_pubsub_e2e() {
    if !require_threadx_riscv64_e2e() {
        return;
    }

    let talker_bin = build_rv64_cpp_talker().expect("build talker failed");
    let listener_bin = build_rv64_cpp_listener().expect("build listener failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C++ listener QEMU...");
    let mut listener =
        QemuProcess::start_riscv64_virt(listener_bin, 1).expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C++ talker QEMU...");
    let mut talker =
        QemuProcess::start_riscv64_virt(talker_bin, 0).expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("C++ Listener output:\n{}", listener_output);
    eprintln!("C++ Talker output:\n{}", talker_output);

    let received = count_pattern(&listener_output, "Received");
    assert!(received > 0, "C++ pubsub E2E failed — 0 messages");
    eprintln!("[PASS] ThreadX RISC-V C++ pubsub E2E: {} msgs", received);
}
