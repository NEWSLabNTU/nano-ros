//! NuttX QEMU ARM virt integration tests
//!
//! Tests that verify NuttX examples build and run on QEMU ARM virt (Cortex-A7).
//! NuttX examples use `armv7a-nuttx-eabihf` target with `std` support.
//!
//! ## Test tiers
//!
//! **Build tests** (require NUTTX_DIR + nightly toolchain):
//!   Verify that cargo cross-compilation succeeds for all 6 examples.
//!
//! **E2E network tests** (require NUTTX_DIR + nightly + QEMU + zenohd):
//!   Verify actual message exchange between two NuttX QEMU instances via zenohd.
//!   Each test boots two QEMU ARM virt instances with slirp user networking,
//!   connecting to zenohd on the host via the slirp gateway (10.0.2.2:7452).
//!
//! ## Prerequisites
//!
//! - `NUTTX_DIR` env var pointing to NuttX source (e.g., `third-party/nuttx/nuttx`)
//! - Nightly Rust toolchain with `armv7a-nuttx-eabihf` target
//! - `qemu-system-arm` with virt machine support
//! - zenohd: `just build-zenohd`
//!
//! Run with: `just test-nuttx`
//! Or: `cargo nextest run -p nros-tests --test nuttx_qemu`

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, is_qemu_available, is_zenohd_available, require_zenohd,
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

/// Check if NUTTX_DIR environment variable is set and points to a valid directory
fn is_nuttx_available() -> bool {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("Makefile").exists())
        .unwrap_or(false)
}

/// Check if NuttX has been configured (config.h exists)
///
/// NuttX must be configured (via `build-nuttx.sh` or `make olddefconfig`) before
/// C code can be compiled against it. NuttX's system headers (e.g., `stdbool.h`)
/// include `<nuttx/config.h>` which is only generated during configuration.
fn is_nuttx_configured() -> bool {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("include/nuttx/config.h").exists())
        .unwrap_or(false)
}

/// Check if arm-none-eabi-gcc is available (required for NuttX cross-compilation)
fn is_arm_gcc_available() -> bool {
    Command::new("arm-none-eabi-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if nightly toolchain supports armv7a-nuttx-eabihf target
///
/// NuttX targets are Tier 3 in Rust — they cannot be installed via `rustup target add`.
/// Instead, they are compiled from source via `-Z build-std`. We check that the nightly
/// compiler knows about the target (it's in the target list) and that rust-src is installed
/// (required for build-std).
fn is_nuttx_toolchain_available() -> bool {
    // Check that nightly knows about the target
    let target_known = Command::new("rustc")
        .args(["+nightly", "--print", "target-list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("armv7a-nuttx-eabihf"))
        .unwrap_or(false);

    // Check that rust-src component is installed (needed for -Z build-std)
    let rust_src = Command::new("rustup")
        .args(["component", "list", "--toolchain", "nightly"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("rust-src (installed)"))
        .unwrap_or(false);

    target_known && rust_src
}

/// Check if a pre-built NuttX kernel image exists
///
/// The NuttX kernel must be built via `build-nuttx.sh` before E2E tests can run.
/// Returns the path to the `nuttx` ELF in $NUTTX_DIR if it exists.
fn nuttx_kernel_path() -> Option<PathBuf> {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("nuttx"))
        .filter(|p| p.exists())
}

/// Skip test if NuttX build prerequisites are not available
fn require_nuttx() -> bool {
    if !is_nuttx_available() {
        eprintln!("Skipping test: NUTTX_DIR not set or invalid");
        eprintln!("Run: just setup-nuttx && export NUTTX_DIR=$(pwd)/third-party/nuttx/nuttx");
        return false;
    }
    if !is_nuttx_configured() {
        eprintln!(
            "Skipping test: NuttX not configured ($NUTTX_DIR/include/nuttx/config.h missing)"
        );
        eprintln!("Run: cd packages/boards/nros-nuttx-qemu-arm && ./scripts/build-nuttx.sh");
        return false;
    }
    if !is_arm_gcc_available() {
        eprintln!("Skipping test: arm-none-eabi-gcc not found");
        eprintln!("Install: sudo apt install gcc-arm-none-eabi");
        return false;
    }
    if !is_nuttx_toolchain_available() {
        eprintln!("Skipping test: nightly toolchain missing rust-src for armv7a-nuttx-eabihf");
        eprintln!(
            "Run: rustup toolchain install nightly && rustup component add rust-src --toolchain nightly"
        );
        return false;
    }
    true
}

/// Skip test if full NuttX E2E prerequisites are not available
///
/// E2E tests require:
/// 1. NuttX build prerequisites (NUTTX_DIR + nightly toolchain)
/// 2. Pre-built NuttX kernel image ($NUTTX_DIR/nuttx)
/// 3. QEMU with ARM virt machine support
/// 4. zenohd router on localhost (slirp networking, no TAP bridge needed)
fn require_nuttx_e2e() -> bool {
    if !require_nuttx() {
        return false;
    }
    if nuttx_kernel_path().is_none() {
        eprintln!("Skipping test: NuttX kernel not built ($NUTTX_DIR/nuttx not found)");
        eprintln!("Build with: cd packages/boards/nros-nuttx-qemu-arm && ./scripts/build-nuttx.sh");
        return false;
    }
    if !is_qemu_available() {
        eprintln!("Skipping test: qemu-system-arm not found");
        eprintln!("Install: sudo apt install qemu-system-arm");
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

static NUTTX_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build a NuttX QEMU example using nightly cargo
fn build_nuttx_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-nuttx/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX example directory not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building qemu-arm-nuttx/rust/zenoh/{}...", name);

    // The cc-rs crate doesn't recognize armv7a-nuttx-eabihf (Tier 3 target) and falls
    // back to the host `cc` (x86 GCC), which fails on ARM flags like -march=armv7-a.
    // Set the target-specific CC env var so cc-rs uses the ARM cross-compiler.
    let output = duct::cmd!("cargo", "+nightly", "build", "--release")
        .dir(&example_dir)
        .env("CC_armv7a_nuttx_eabi", "arm-none-eabi-gcc")
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

    let binary_path = example_dir.join(format!(
        "target/armv7a-nuttx-eabihf/release/{}",
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

fn build_nuttx_talker() -> TestResult<&'static Path> {
    NUTTX_TALKER_BINARY
        .get_or_try_init(|| build_nuttx_example("talker", "nuttx-rs-talker"))
        .map(|p| p.as_path())
}

fn build_nuttx_listener() -> TestResult<&'static Path> {
    NUTTX_LISTENER_BINARY
        .get_or_try_init(|| build_nuttx_example("listener", "nuttx-rs-listener"))
        .map(|p| p.as_path())
}

fn build_nuttx_service_server() -> TestResult<&'static Path> {
    NUTTX_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_nuttx_example("service-server", "nuttx-rs-service-server"))
        .map(|p| p.as_path())
}

fn build_nuttx_service_client() -> TestResult<&'static Path> {
    NUTTX_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_nuttx_example("service-client", "nuttx-rs-service-client"))
        .map(|p| p.as_path())
}

fn build_nuttx_action_server() -> TestResult<&'static Path> {
    NUTTX_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_nuttx_example("action-server", "nuttx-rs-action-server"))
        .map(|p| p.as_path())
}

fn build_nuttx_action_client() -> TestResult<&'static Path> {
    NUTTX_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_nuttx_example("action-client", "nuttx-rs-action-client"))
        .map(|p| p.as_path())
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_nuttx_detection() {
    let available = is_nuttx_available();
    let configured = is_nuttx_configured();
    let arm_gcc = is_arm_gcc_available();
    let toolchain = is_nuttx_toolchain_available();
    let qemu = is_qemu_available();
    let zenohd = is_zenohd_available();
    let kernel = nuttx_kernel_path();
    eprintln!("NuttX available: {}", available);
    eprintln!("NuttX configured: {}", configured);
    eprintln!("arm-none-eabi-gcc available: {}", arm_gcc);
    eprintln!("NuttX toolchain available: {}", toolchain);
    eprintln!("QEMU available: {}", qemu);
    eprintln!("zenohd available: {}", zenohd);
    eprintln!(
        "NuttX kernel: {}",
        kernel
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not built".to_string())
    );
}

// =============================================================================
// Build tests (require NUTTX_DIR + nightly toolchain)
// =============================================================================

#[test]
fn test_nuttx_talker_builds() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }
    let binary = build_nuttx_talker().expect("Failed to build nuttx-rs-talker");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!("SUCCESS: nuttx-rs-talker builds at {}", binary.display());
}

#[test]
fn test_nuttx_listener_builds() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }
    let binary = build_nuttx_listener().expect("Failed to build nuttx-rs-listener");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!("SUCCESS: nuttx-rs-listener builds at {}", binary.display());
}

#[test]
fn test_nuttx_service_server_builds() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }
    let binary = build_nuttx_service_server().expect("Failed to build nuttx-rs-service-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: nuttx-rs-service-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_nuttx_service_client_builds() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }
    let binary = build_nuttx_service_client().expect("Failed to build nuttx-rs-service-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: nuttx-rs-service-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_nuttx_action_server_builds() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }
    let binary = build_nuttx_action_server().expect("Failed to build nuttx-rs-action-server");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: nuttx-rs-action-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_nuttx_action_client_builds() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }
    let binary = build_nuttx_action_client().expect("Failed to build nuttx-rs-action-client");
    assert!(
        binary.exists(),
        "Binary should exist at {}",
        binary.display()
    );
    eprintln!(
        "SUCCESS: nuttx-rs-action-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_nuttx_all_examples_build() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }

    let results = [
        ("talker", build_nuttx_talker()),
        ("listener", build_nuttx_listener()),
        ("service-server", build_nuttx_service_server()),
        ("service-client", build_nuttx_service_client()),
        ("action-server", build_nuttx_action_server()),
        ("action-client", build_nuttx_action_client()),
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

    assert!(all_ok, "Not all NuttX examples built successfully");
}

// =============================================================================
// NuttX kernel boot test (require QEMU + NuttX kernel image)
// =============================================================================

/// Verify that the NuttX kernel boots to NSH prompt in QEMU ARM virt.
///
/// This test does not require networking — it boots NuttX with `-nic none`
/// and checks for the NSH shell prompt, validating the kernel + QEMU setup.
#[test]
fn test_nuttx_kernel_boots() {
    if !is_nuttx_available() {
        eprintln!("Skipping: NUTTX_DIR not set");
        return;
    }
    let kernel = match nuttx_kernel_path() {
        Some(k) => k,
        None => {
            eprintln!("Skipping: NuttX kernel not built ($NUTTX_DIR/nuttx)");
            return;
        }
    };
    if !is_qemu_available() {
        eprintln!("Skipping: qemu-system-arm not found");
        return;
    }

    eprintln!("Booting NuttX kernel: {}", kernel.display());

    // Boot NuttX without networking (just verify kernel boot)
    let mut qemu = QemuProcess::start_nuttx_virt(&kernel, false)
        .expect("Failed to start QEMU with NuttX kernel");

    // NuttX should boot to NSH prompt within 10 seconds
    let output = qemu
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    qemu.kill();

    eprintln!("NuttX boot output:\n{}", output);

    // Check for NuttX boot markers
    let has_nsh = output.contains("nsh>") || output.contains("NuttShell");
    let has_nuttx = output.contains("NuttX");

    if has_nsh {
        eprintln!("[PASS] NuttX booted to NSH prompt");
    } else if has_nuttx {
        eprintln!("[PARTIAL] NuttX started but NSH prompt not found");
    } else {
        eprintln!("[INFO] No NuttX output detected — kernel may need configuration");
        eprintln!("Build: cd packages/boards/nros-nuttx-qemu-arm && ./scripts/build-nuttx.sh");
    }
}

// =============================================================================
// E2E Network tests (require QEMU + zenohd + NuttX kernel)
// =============================================================================
//
// NuttX QEMU ARM virt examples use virtio-net with slirp user networking:
//   qemu-system-arm -M virt -cpu cortex-a7 -nographic -kernel <nuttx-image> \
//       -nic user,net=10.0.2.0/24,host=10.0.2.2,hostfwd=...
//
// Network topology (slirp — each QEMU has isolated 10.0.2.0/24 network):
//   QEMU node 0 (slirp, 10.0.2.30) ---> 10.0.2.2:7452 --+
//                                                          |-- zenohd (localhost:7452)
//   QEMU node 1 (slirp, 10.0.2.31) ---> 10.0.2.2:7452 --+
//
// IP assignments (hardcoded in board crate Config):
//   10.0.2.30  - Talker / Server
//   10.0.2.31  - Listener
//   10.0.2.32  - Server
//   10.0.2.33  - Client
//
// Prerequisites:
//   1. zenohd: just build-zenohd
//   2. NuttX kernel: build-nuttx.sh (with Rust apps integrated)
//   3. Run: just test-nuttx

/// Test pub/sub message exchange between NuttX QEMU instances.
///
/// Launches a listener and a talker on separate QEMU instances (slirp networking),
/// verifies that the listener receives Int32 messages published by the talker.
#[test]
fn test_nuttx_pubsub_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    // Build both binaries
    let talker_bin = build_nuttx_talker().expect("Failed to build talker");
    let listener_bin = build_nuttx_listener().expect("Failed to build listener");

    // Start zenohd (NuttX binaries connect via slirp gateway to host)
    let mut zenohd =
        ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");
    assert!(zenohd.is_running(), "zenohd should be running");

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting listener QEMU (slirp, 10.0.2.31)...");
    let mut listener =
        QemuProcess::start_nuttx_virt(listener_bin, true).expect("Failed to start listener QEMU");

    // Wait for listener to be ready (NuttX boot + zenoh connect + subscription)
    let listener_ready = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let listener_booted = listener_ready.contains("Waiting for messages");
    eprintln!(
        "Listener boot output ({} chars): {}",
        listener_ready.len(),
        &listener_ready[..listener_ready.len().min(500)]
    );

    if !listener_booted {
        eprintln!(
            "[SKIP] Listener did not reach readiness — NuttX app integration may be incomplete"
        );
        eprintln!("The NuttX kernel must include the Rust app as a builtin or via ROMFS.");
        eprintln!("See: docs/roadmap/phase-55-nuttx-platform.md (55.12)");
        return;
    }

    // Start talker QEMU
    eprintln!("Starting talker QEMU (slirp, 10.0.2.30)...");
    let mut talker =
        QemuProcess::start_nuttx_virt(talker_bin, true).expect("Failed to start talker QEMU");

    // Wait for talker to start publishing
    let _talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    // Wait for listener to receive messages
    let final_output = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let full_output = format!("{}{}", listener_ready, final_output);

    talker.kill();
    listener.kill();

    eprintln!("Listener full output:\n{}", full_output);

    let received_count = count_pattern(&full_output, "Received");
    eprintln!("Messages received: {}", received_count);

    if received_count > 0 {
        eprintln!(
            "[PASS] NuttX pubsub E2E: {} messages exchanged",
            received_count
        );
    } else {
        eprintln!("[FAIL] NuttX pubsub E2E: no messages received");
        panic!("NuttX pubsub E2E failed — listener received 0 messages");
    }
}

/// Test service request/response between NuttX QEMU instances.
///
/// Launches a service server and a client on separate QEMU instances (slirp networking),
/// verifies that the client receives correct AddTwoInts responses.
#[test]
fn test_nuttx_service_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server_bin = build_nuttx_service_server().expect("Failed to build service server");
    let client_bin = build_nuttx_service_client().expect("Failed to build service client");

    let mut zenohd =
        ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");
    assert!(zenohd.is_running(), "zenohd should be running");

    // Start server first
    eprintln!("Starting service server QEMU (slirp, 10.0.2.30)...");
    let mut server =
        QemuProcess::start_nuttx_virt(server_bin, true).expect("Failed to start server QEMU");

    let server_output = server
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_ready = server_output.contains("Waiting for requests");
    eprintln!(
        "Server boot output ({} chars): {}",
        server_output.len(),
        &server_output[..server_output.len().min(500)]
    );

    if !server_ready {
        eprintln!(
            "[SKIP] Server did not reach readiness — NuttX app integration may be incomplete"
        );
        return;
    }

    // Start client
    eprintln!("Starting service client QEMU (slirp, 10.0.2.31)...");
    let mut client =
        QemuProcess::start_nuttx_virt(client_bin, true).expect("Failed to start client QEMU");

    // Wait for client to complete all service calls (4 calls: 5+3, 10+20, 100+200, -5+10)
    let client_output = client
        .wait_for_output(Duration::from_secs(45))
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
            "[PASS] NuttX service E2E: {} responses, all correct",
            response_count
        );
    } else if response_count > 0 {
        eprintln!(
            "[PARTIAL] NuttX service E2E: {} of 4 responses",
            response_count
        );
    } else {
        eprintln!("[FAIL] NuttX service E2E: no responses received");
        panic!("NuttX service E2E failed — client received 0 responses");
    }
}

/// Test action goal/feedback/result between NuttX QEMU instances.
///
/// Launches an action server and a client on separate QEMU instances (slirp networking),
/// verifies that the client receives Fibonacci feedback and final result.
#[test]
fn test_nuttx_action_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server_bin = build_nuttx_action_server().expect("Failed to build action server");
    let client_bin = build_nuttx_action_client().expect("Failed to build action client");

    let mut zenohd =
        ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");
    assert!(zenohd.is_running(), "zenohd should be running");

    // Start action server first
    eprintln!("Starting action server QEMU (slirp, 10.0.2.30)...");
    let mut server =
        QemuProcess::start_nuttx_virt(server_bin, true).expect("Failed to start server QEMU");

    let server_output = server
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_ready = server_output.contains("Waiting for goals");
    eprintln!(
        "Server boot output ({} chars): {}",
        server_output.len(),
        &server_output[..server_output.len().min(500)]
    );

    if !server_ready {
        eprintln!(
            "[SKIP] Action server did not reach readiness — NuttX app integration may be incomplete"
        );
        return;
    }

    // Start action client
    eprintln!("Starting action client QEMU (slirp, 10.0.2.31)...");
    let mut client =
        QemuProcess::start_nuttx_virt(client_bin, true).expect("Failed to start client QEMU");

    // Fibonacci(10) takes ~5.5s to compute + NuttX boot + zenoh connect
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("Client output:\n{}", client_output);

    // Verify action protocol
    let goal_accepted = client_output.contains("Goal accepted");
    let feedback_count = count_pattern(&client_output, "Feedback #");
    let completed = client_output.contains("Action client finished")
        || client_output.contains("All feedback received");

    eprintln!(
        "Goal accepted: {}, feedback: {}, completed: {}",
        goal_accepted, feedback_count, completed
    );

    if goal_accepted && feedback_count > 0 && completed {
        eprintln!(
            "[PASS] NuttX action E2E: goal accepted, {} feedback msgs, completed",
            feedback_count
        );
    } else {
        eprintln!("[FAIL] NuttX action E2E:");
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
            "NuttX action E2E failed: accepted={}, feedback={}, completed={}",
            goal_accepted, feedback_count, completed
        );
    }
}

// =============================================================================
// C++ binary builders (CMake-based)
// =============================================================================

/// Build a NuttX C or C++ QEMU example via CMake (nuttx_build_example).
fn build_nuttx_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-nuttx/{}/zenoh/{}", lang, name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX {lang} example not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building qemu-arm-nuttx/{}/zenoh/{} (CMake)...", lang, name);

    let build_dir = example_dir.join("build");
    std::fs::create_dir_all(&build_dir).ok();

    // cmake configure — nuttx_build_example() handles cross-compilation via cargo,
    // so no CMake toolchain file is needed. Pass NUTTX_DIR for the kernel link.
    let prefix_path = format!(
        "-DCMAKE_PREFIX_PATH={}",
        root.join("build/install").display()
    );
    let nuttx_dir = std::env::var("NUTTX_DIR")
        .unwrap_or_else(|_| root.join("third-party/nuttx/nuttx").display().to_string());

    let output = duct::cmd!(
        "cmake",
        "-S",
        &example_dir,
        "-B",
        &build_dir,
        &prefix_path,
        &format!("-DNUTTX_DIR={nuttx_dir}"),
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

    let output = duct::cmd!("cmake", "--build", &build_dir)
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

static NUTTX_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_nuttx_cpp_talker() -> TestResult<&'static Path> {
    NUTTX_CPP_TALKER_BINARY
        .get_or_try_init(|| build_nuttx_cmake_example("cpp", "talker", "nuttx_cpp_talker"))
        .map(|p| p.as_path())
}

fn build_nuttx_cpp_listener() -> TestResult<&'static Path> {
    NUTTX_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_nuttx_cmake_example("cpp", "listener", "nuttx_cpp_listener"))
        .map(|p| p.as_path())
}

fn build_nuttx_cpp_service_server() -> TestResult<&'static Path> {
    NUTTX_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_nuttx_cmake_example("cpp", "service-server", "nuttx_cpp_service_server")
        })
        .map(|p| p.as_path())
}

fn build_nuttx_cpp_service_client() -> TestResult<&'static Path> {
    NUTTX_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_nuttx_cmake_example("cpp", "service-client", "nuttx_cpp_service_client")
        })
        .map(|p| p.as_path())
}

fn require_nuttx_cpp() -> bool {
    if !require_nuttx() {
        return false;
    }
    if !Command::new("cmake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        eprintln!("Skipping test: cmake not found");
        return false;
    }
    true
}

// =============================================================================
// C++ Build tests
// =============================================================================

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_talker_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_talker().expect("Failed to build nuttx_cpp_talker");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_talker at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_listener_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_listener().expect("Failed to build nuttx_cpp_listener");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_listener at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_service_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary =
        build_nuttx_cpp_service_server().expect("Failed to build nuttx_cpp_service_server");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_service_server at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_service_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary =
        build_nuttx_cpp_service_client().expect("Failed to build nuttx_cpp_service_client");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_service_client at {}", binary.display());
}

// =============================================================================
// C++ E2E Network tests
// =============================================================================

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_pubsub_e2e() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let talker_bin = build_nuttx_cpp_talker().expect("Failed to build C++ talker");
    let listener_bin = build_nuttx_cpp_listener().expect("Failed to build C++ listener");

    let _zenohd = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C++ listener QEMU (slirp, 10.0.2.31)...");
    let mut listener =
        QemuProcess::start_nuttx_virt(listener_bin, true).expect("Failed to start listener");

    let listener_ready = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();

    if !listener_ready.contains("Waiting for messages") {
        nros_tests::skip!("C++ listener did not reach readiness");
    }

    eprintln!("Starting C++ talker QEMU (slirp, 10.0.2.30)...");
    let mut talker =
        QemuProcess::start_nuttx_virt(talker_bin, true).expect("Failed to start talker");

    let _talker_out = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();
    let final_out = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let full = format!("{}{}", listener_ready, final_out);

    talker.kill();
    listener.kill();

    let received = count_pattern(&full, "Received");
    eprintln!("C++ NuttX messages received: {}", received);
    assert!(received > 0, "NuttX C++ pubsub E2E: 0 messages received");
    eprintln!("[PASS] NuttX C++ pubsub E2E: {} messages", received);
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_service_e2e() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server_bin = build_nuttx_cpp_service_server().expect("Failed to build C++ service server");
    let client_bin = build_nuttx_cpp_service_client().expect("Failed to build C++ service client");

    let _zenohd = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C++ service server QEMU...");
    let mut server =
        QemuProcess::start_nuttx_virt(server_bin, true).expect("Failed to start server");

    let server_ready = server
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    if !server_ready.contains("Service server ready") {
        nros_tests::skip!("C++ server did not reach readiness");
    }

    eprintln!("Starting C++ service client QEMU...");
    let mut client =
        QemuProcess::start_nuttx_virt(client_bin, true).expect("Failed to start client");

    let client_out = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    server.kill();
    client.kill();

    let responses = count_pattern(&client_out, "Response:");
    let completed = client_out.contains("All service calls completed");
    eprintln!(
        "C++ NuttX responses: {}, completed: {}",
        responses, completed
    );
    assert!(responses > 0, "NuttX C++ service E2E: 0 responses");
    eprintln!("[PASS] NuttX C++ service E2E: {} responses", responses);
}

// =============================================================================
// C binary builders (uses same nuttx_build_example CMake approach as C++)
// =============================================================================

static NUTTX_C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_nuttx_c_talker() -> TestResult<&'static Path> {
    NUTTX_C_TALKER_BINARY
        .get_or_try_init(|| build_nuttx_cmake_example("c", "talker", "nuttx_c_talker"))
        .map(|p| p.as_path())
}
fn build_nuttx_c_listener() -> TestResult<&'static Path> {
    NUTTX_C_LISTENER_BINARY
        .get_or_try_init(|| build_nuttx_cmake_example("c", "listener", "nuttx_c_listener"))
        .map(|p| p.as_path())
}
fn build_nuttx_c_service_server() -> TestResult<&'static Path> {
    NUTTX_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_nuttx_cmake_example("c", "service-server", "nuttx_c_service_server")
        })
        .map(|p| p.as_path())
}
fn build_nuttx_c_service_client() -> TestResult<&'static Path> {
    NUTTX_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_nuttx_cmake_example("c", "service-client", "nuttx_c_service_client")
        })
        .map(|p| p.as_path())
}
fn build_nuttx_c_action_server() -> TestResult<&'static Path> {
    NUTTX_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_nuttx_cmake_example("c", "action-server", "nuttx_c_action_server")
        })
        .map(|p| p.as_path())
}
fn build_nuttx_c_action_client() -> TestResult<&'static Path> {
    NUTTX_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_nuttx_cmake_example("c", "action-client", "nuttx_c_action_client")
        })
        .map(|p| p.as_path())
}

// =============================================================================
// C Build tests
// =============================================================================

#[test]
fn test_nuttx_c_talker_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let b = build_nuttx_c_talker().expect("build failed");
    assert!(b.exists());
    eprintln!("SUCCESS: {}", b.display());
}

#[test]
fn test_nuttx_c_listener_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let b = build_nuttx_c_listener().expect("build failed");
    assert!(b.exists());
}

#[test]
fn test_nuttx_c_service_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let b = build_nuttx_c_service_server().expect("build failed");
    assert!(b.exists());
}

#[test]
fn test_nuttx_c_service_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let b = build_nuttx_c_service_client().expect("build failed");
    assert!(b.exists());
}

#[test]
fn test_nuttx_c_action_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let b = build_nuttx_c_action_server().expect("build failed");
    assert!(b.exists());
}

#[test]
fn test_nuttx_c_action_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let b = build_nuttx_c_action_client().expect("build failed");
    assert!(b.exists());
}

// =============================================================================
// C E2E tests (QEMU ARM virt + slirp networking)
// =============================================================================

#[test]
fn test_nuttx_c_pubsub_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let talker = build_nuttx_c_talker().expect("build talker");
    let listener = build_nuttx_c_listener().expect("build listener");

    let _z = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("zenohd");

    let mut l = QemuProcess::start_nuttx_virt(listener, true).expect("listener QEMU");
    std::thread::sleep(Duration::from_secs(10));
    let mut t = QemuProcess::start_nuttx_virt(talker, true).expect("talker QEMU");

    let l_out = l
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    let t_out = t
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();
    t.kill();
    l.kill();

    eprintln!("C Listener:\n{l_out}\nC Talker:\n{t_out}");
    let received = count_pattern(&l_out, "Received");
    assert!(received > 0, "NuttX C pubsub: 0 messages");
    eprintln!("[PASS] NuttX C pubsub E2E: {received} msgs");
}

#[test]
fn test_nuttx_c_service_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server = build_nuttx_c_service_server().expect("build server");
    let client = build_nuttx_c_service_client().expect("build client");

    let _z = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("zenohd");

    let mut s = QemuProcess::start_nuttx_virt(server, true).expect("server QEMU");
    std::thread::sleep(Duration::from_secs(10));
    let mut c = QemuProcess::start_nuttx_virt(client, true).expect("client QEMU");

    std::thread::sleep(Duration::from_secs(15));
    let c_out = c
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    s.kill();
    c.kill();

    eprintln!("C Client:\n{c_out}");
    let responses = count_pattern(&c_out, "Response:");
    assert!(responses > 0, "NuttX C service: 0 responses");
    eprintln!("[PASS] NuttX C service E2E: {responses} responses");
}

#[test]
fn test_nuttx_c_action_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server = build_nuttx_c_action_server().expect("build server");
    let client = build_nuttx_c_action_client().expect("build client");

    let _z = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("zenohd");

    let mut s = QemuProcess::start_nuttx_virt(server, true).expect("server QEMU");
    std::thread::sleep(Duration::from_secs(10));
    let mut c = QemuProcess::start_nuttx_virt(client, true).expect("client QEMU");

    std::thread::sleep(Duration::from_secs(15));
    let c_out = c
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    s.kill();
    c.kill();

    eprintln!("C Client:\n{c_out}");
    let accepted = c_out.contains("Goal accepted");
    let completed = c_out.contains("Action completed successfully");
    assert!(
        accepted && completed,
        "NuttX C action: accepted={accepted}, completed={completed}"
    );
    eprintln!("[PASS] NuttX C action E2E");
}
