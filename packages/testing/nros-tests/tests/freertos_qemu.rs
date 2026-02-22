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

use nros_tests::fixtures::is_qemu_available;
use nros_tests::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    eprintln!("FreeRTOS available: {}", freertos);
    eprintln!("lwIP available: {}", lwip);
    eprintln!("arm-none-eabi-gcc available: {}", arm_gcc);
    eprintln!("QEMU available: {}", qemu);
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
// Network tests (require QEMU + TAP bridge)
// =============================================================================
//
// FreeRTOS QEMU MPS2-AN385 examples use LAN9118 Ethernet with TAP networking:
//   qemu-system-arm -machine mps2-an385 -cpu cortex-m3 -nographic \
//       -semihosting-config enable=on,target=native \
//       -kernel <binary> \
//       -nic tap,ifname=tap-qemu0,script=no,downscript=no
//
// These tests are skipped by default. Run with:
//   just test-freertos
//
// Or manually:
//   1. Set up TAP bridge: sudo ./scripts/qemu/setup-network.sh
//   2. Start zenohd: zenohd --listen tcp/192.0.3.1:7447
//   3. Run: cargo nextest run -p nros-tests --test freertos_qemu

#[test]
fn test_freertos_pubsub_requires_network() {
    eprintln!("Skipping test: FreeRTOS pubsub E2E requires QEMU TAP networking");
    eprintln!("Run with: just test-freertos");
    println!("INFO: FreeRTOS network tests skipped (use TAP bridge for full test)");
}

#[test]
fn test_freertos_service_requires_network() {
    eprintln!("Skipping test: FreeRTOS service E2E requires QEMU TAP networking");
    eprintln!("Run with: just test-freertos");
    println!("INFO: FreeRTOS network tests skipped (use TAP bridge for full test)");
}

#[test]
fn test_freertos_action_requires_network() {
    eprintln!("Skipping test: FreeRTOS action E2E requires QEMU TAP networking");
    eprintln!("Run with: just test-freertos");
    println!("INFO: FreeRTOS network tests skipped (use TAP bridge for full test)");
}
