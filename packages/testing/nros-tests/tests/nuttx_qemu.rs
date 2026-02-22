//! NuttX QEMU ARM virt integration tests
//!
//! Tests that verify NuttX examples build and run on QEMU ARM virt (Cortex-A7).
//! NuttX examples use `armv7a-nuttx-eabi` target with `std` support.
//!
//! Prerequisites:
//! - `NUTTX_DIR` env var pointing to NuttX source (e.g., `external/nuttx`)
//! - Nightly Rust toolchain with `armv7a-nuttx-eabi` target
//! - `qemu-system-arm` with virt machine support
//!
//! Run with: `just test-nuttx`
//! Or: `cargo nextest run -p nros-tests --test nuttx_qemu`

use nros_tests::fixtures::is_qemu_available;
use nros_tests::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Check if nightly toolchain has armv7a-nuttx-eabi target
fn is_nuttx_toolchain_available() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed", "--toolchain", "nightly"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("armv7a-nuttx-eabi"))
        .unwrap_or(false)
}

/// Skip test if NuttX prerequisites are not available
fn require_nuttx() -> bool {
    if !is_nuttx_available() {
        eprintln!("Skipping test: NUTTX_DIR not set or invalid");
        eprintln!("Run: just setup-nuttx && export NUTTX_DIR=$(pwd)/external/nuttx");
        return false;
    }
    if !is_nuttx_toolchain_available() {
        eprintln!("Skipping test: armv7a-nuttx-eabi target not installed");
        eprintln!("Run: rustup target add armv7a-nuttx-eabi --toolchain nightly");
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

    let output = duct::cmd!("cargo", "+nightly", "build", "--release")
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

    let binary_path = example_dir.join(format!("target/armv7a-nuttx-eabi/release/{}", binary_name));

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
    let toolchain = is_nuttx_toolchain_available();
    let qemu = is_qemu_available();
    eprintln!("NuttX available: {}", available);
    eprintln!("NuttX toolchain available: {}", toolchain);
    eprintln!("QEMU available: {}", qemu);
}

// =============================================================================
// Build tests (require NUTTX_DIR + nightly toolchain)
// =============================================================================

#[test]
fn test_nuttx_talker_builds() {
    if !require_nuttx() {
        return;
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
        return;
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
        return;
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
        return;
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
        return;
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
        return;
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
        return;
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
// Network tests (require QEMU + TAP bridge)
// =============================================================================
//
// NuttX QEMU ARM virt examples use virtio-net with TAP networking:
//   qemu-system-arm -M virt -cpu cortex-a7 -nographic -kernel <binary> \
//       -nic tap,ifname=tap-qemu0,script=no,downscript=no
//
// These tests are skipped by default. Run with:
//   just test-nuttx
//
// Or manually:
//   1. Set up TAP bridge: sudo ./scripts/qemu/setup-network.sh
//   2. Start zenohd: zenohd --listen tcp/192.0.3.1:7447
//   3. Run: cargo nextest run -p nros-tests --test nuttx_qemu

#[test]
fn test_nuttx_pubsub_requires_network() {
    eprintln!("Skipping test: NuttX pubsub E2E requires QEMU TAP networking");
    eprintln!("Run with: just test-nuttx");
    println!("INFO: NuttX network tests skipped (use TAP bridge for full test)");
}

#[test]
fn test_nuttx_service_requires_network() {
    eprintln!("Skipping test: NuttX service E2E requires QEMU TAP networking");
    eprintln!("Run with: just test-nuttx");
    println!("INFO: NuttX network tests skipped (use TAP bridge for full test)");
}

#[test]
fn test_nuttx_action_requires_network() {
    eprintln!("Skipping test: NuttX action E2E requires QEMU TAP networking");
    eprintln!("Run with: just test-nuttx");
    println!("INFO: NuttX network tests skipped (use TAP bridge for full test)");
}
