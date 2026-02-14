//! Emulator tests for nano-ros
//!
//! Tests that run on QEMU Cortex-M3 emulator without physical hardware.
//! These verify CDR serialization, Node API, and type metadata work on embedded targets.
//!
//! Run with: `cargo test -p nano-ros-tests --test emulator -- --nocapture`
//! Or: `just test-rust-emulator`
//!
//! ## BSP Tests
//!
//! The BSP (Board Support Package) tests verify the simplified nano-ros-platform-qemu API:
//! - `just test-qemu-bsp` - Run BSP build and startup tests

use nano_ros_tests::fixtures::{
    QemuProcess, build_example, build_qemu_bsp_listener, build_qemu_bsp_talker,
    build_qemu_rs_listener, build_qemu_rs_talker, is_arm_toolchain_available, is_qemu_available,
    parse_test_results, qemu_binary, require_docker_compose, require_zenoh_pico_arm,
};
use nano_ros_tests::{assert_output_contains, assert_output_excludes, count_pattern, project_root};
use rstest::rstest;
use std::path::PathBuf;
use std::time::Duration;

/// Skip test if QEMU is not available
fn require_qemu() {
    if !is_qemu_available() {
        eprintln!("Skipping test: qemu-system-arm not found");
        return;
    }
}

/// Skip test if ARM toolchain is not available
fn require_arm_toolchain() {
    if !is_arm_toolchain_available() {
        eprintln!("Skipping test: thumbv7m-none-eabi target not installed");
        return;
    }
}

// =============================================================================
// QEMU Cortex-M3 Tests
// =============================================================================

#[rstest]
fn test_qemu_cdr_serialization(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify CDR serialization tests passed
    assert_output_contains(
        &output,
        &[
            "[PASS] Int32 roundtrip",
            "[PASS] Float64 roundtrip",
            "[PASS] Time roundtrip",
            "[PASS] CDR header",
        ],
    );

    // Verify no test failures
    assert_output_excludes(&output, &["[FAIL]"]);
}

#[rstest]
fn test_qemu_node_api(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify Node API tests passed
    assert_output_contains(
        &output,
        &[
            "[PASS] Node creation",
            "[PASS] Node publisher",
            "[PASS] Node subscriber",
            "[PASS] Node serialize",
        ],
    );
}

#[rstest]
fn test_qemu_type_metadata(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify type metadata test passed
    assert_output_contains(&output, &["[PASS] Type names"]);
}

#[rstest]
fn test_qemu_all_tests_pass(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Parse and verify results
    let (passed, failed) = parse_test_results(&output);

    assert!(
        passed >= 9,
        "Expected at least 9 tests to pass, got {}",
        passed
    );
    assert_eq!(
        failed, 0,
        "Expected no failures, got {}. Output:\n{}",
        failed, output
    );

    // Verify completion message
    assert_output_contains(&output, &["All tests passed"]);
}

#[rstest]
fn test_qemu_output_format(qemu_binary: PathBuf) {
    require_qemu();
    require_arm_toolchain();

    let mut qemu = QemuProcess::start_cortex_m3(&qemu_binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    // Verify output has expected format
    let pass_count = count_pattern(&output, "[PASS]");
    let fail_count = count_pattern(&output, "[FAIL]");

    eprintln!("Test results: {} passed, {} failed", pass_count, fail_count);
    eprintln!("Output:\n{}", output);

    assert!(pass_count > 0, "No [PASS] markers found in output");
}

// =============================================================================
// QEMU Availability Tests
// =============================================================================

#[test]
fn test_qemu_detection() {
    let available = is_qemu_available();
    eprintln!("QEMU available: {}", available);
    // This test just verifies the detection works, doesn't require QEMU
}

#[test]
fn test_arm_toolchain_detection() {
    let available = is_arm_toolchain_available();
    eprintln!("ARM toolchain available: {}", available);
    // This test just verifies the detection works
}

// =============================================================================
// QEMU BSP Tests (Phase 17.7)
// =============================================================================
//
// Tests for the simplified nano-ros-platform-qemu API (Board Support Package).
// These examples use a higher-level API than the rs-* examples.

/// Test that qemu-bsp-talker builds successfully
#[test]
fn test_qemu_bsp_talker_builds() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let result = build_qemu_bsp_talker();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!("SUCCESS: qemu-bsp-talker builds at {}", binary.display());
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!("Fix with: sudo rm -rf examples/qemu/bsp-talker/target");
                eprintln!("Skipping test...");
            } else {
                panic!("qemu-bsp-talker build failed: {:?}", e);
            }
        }
    }
}

/// Test that qemu-bsp-listener builds successfully
#[test]
fn test_qemu_bsp_listener_builds() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let result = build_qemu_bsp_listener();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!("SUCCESS: qemu-bsp-listener builds at {}", binary.display());
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!("Fix with: sudo rm -rf examples/qemu/bsp-listener/target");
                eprintln!("Skipping test...");
            } else {
                panic!("qemu-bsp-listener build failed: {:?}", e);
            }
        }
    }
}

// =============================================================================
// BSP Network Tests (Require Docker or TAP)
// =============================================================================
//
// The BSP examples use the MPS2-AN385 machine with LAN9118 Ethernet, which
// requires network setup. These tests are skipped by default.
//
// To run BSP network tests:
//   just test-rust-qemu-baremetal-bsp  (uses Docker)
//
// Or manually with TAP interface:
//   sudo ./scripts/qemu/setup-network.sh
//   zenohd --listen tcp/0.0.0.0:7447
//   ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary <path>

/// Test that qemu-bsp-talker starts (requires Docker/TAP networking)
///
/// NOTE: This test is skipped by default as it requires network setup.
/// Use `just test-rust-qemu-baremetal-bsp` for the full Docker-based test.
#[test]
fn test_qemu_bsp_talker_starts() {
    // BSP examples require MPS2-AN385 with networking, which isn't available
    // in the standard test environment. The Docker-based test handles this.
    eprintln!("Skipping test: BSP start tests require Docker or TAP networking");
    eprintln!("Run with: just test-rust-qemu-baremetal-bsp");
    println!("INFO: BSP network tests skipped (use Docker for full test)");
}

/// Test that qemu-bsp-listener starts (requires Docker/TAP networking)
///
/// NOTE: This test is skipped by default as it requires network setup.
/// Use `just test-rust-qemu-baremetal-bsp` for the full Docker-based test.
#[test]
fn test_qemu_bsp_listener_starts() {
    // BSP examples require MPS2-AN385 with networking, which isn't available
    // in the standard test environment. The Docker-based test handles this.
    eprintln!("Skipping test: BSP start tests require Docker or TAP networking");
    eprintln!("Run with: just test-rust-qemu-baremetal-bsp");
    println!("INFO: BSP network tests skipped (use Docker for full test)");
}

/// Test that both BSP binaries can be built in sequence
///
/// This verifies the build system handles multiple BSP binaries correctly.
#[test]
fn test_qemu_bsp_both_build() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let talker = build_qemu_bsp_talker();
    let listener = build_qemu_bsp_listener();

    // Handle permission errors gracefully
    let has_perm_error = |e: &dyn std::fmt::Debug| format!("{:?}", e).contains("Permission denied");

    match (&talker, &listener) {
        (Ok(talker_path), Ok(listener_path)) => {
            // Verify they're different binaries
            assert_ne!(
                talker_path.file_name(),
                listener_path.file_name(),
                "Talker and listener should be different binaries"
            );

            println!("SUCCESS: Both BSP binaries build correctly");
            println!("  Talker:   {}", talker_path.display());
            println!("  Listener: {}", listener_path.display());
        }
        (Err(e), _) if has_perm_error(e) => {
            eprintln!("Talker build failed due to permission issues");
            eprintln!("Fix with: sudo rm -rf examples/qemu/bsp-talker/target");
            eprintln!("Skipping test...");
        }
        (_, Err(e)) if has_perm_error(e) => {
            eprintln!("Listener build failed due to permission issues");
            eprintln!("Fix with: sudo rm -rf examples/qemu/bsp-listener/target");
            eprintln!("Skipping test...");
        }
        (Err(e), _) => panic!("BSP talker build failed: {:?}", e),
        (_, Err(e)) => panic!("BSP listener build failed: {:?}", e),
    }
}

// =============================================================================
// QEMU rs-talker/rs-listener Tests (Phase 17.10.2)
// =============================================================================
//
// The rs-talker and rs-listener examples use MPS2-AN385 with LAN9118 Ethernet
// networking. Build verification can run without Docker; E2E communication
// requires the Docker Compose infrastructure in tests/qemu-baremetal/.

/// Test that qemu-rs-talker builds for thumbv7m-none-eabi
#[test]
fn test_qemu_rs_talker_builds() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let result = build_qemu_rs_talker();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            eprintln!("SUCCESS: qemu-rs-talker builds at {}", binary.display());
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!("Fix with: sudo rm -rf examples/qemu/rs-talker/target");
                eprintln!("Skipping test...");
            } else {
                panic!("qemu-rs-talker build failed: {:?}", e);
            }
        }
    }
}

/// Test that qemu-rs-listener builds for thumbv7m-none-eabi
#[test]
fn test_qemu_rs_listener_builds() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let result = build_qemu_rs_listener();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            eprintln!("SUCCESS: qemu-rs-listener builds at {}", binary.display());
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!("Fix with: sudo rm -rf examples/qemu/rs-listener/target");
                eprintln!("Skipping test...");
            } else {
                panic!("qemu-rs-listener build failed: {:?}", e);
            }
        }
    }
}

/// Test QEMU rs-talker → rs-listener end-to-end communication via Docker
///
/// This test uses the Docker Compose infrastructure at `tests/qemu-baremetal/`
/// to run zenohd, talker, and listener in separate containers with TAP networking.
///
/// Prerequisites:
/// - Docker with compose plugin
/// - ARM toolchain (`thumbv7m-none-eabi`)
/// - zenoh-pico ARM library (`just build-zenoh-pico-arm`)
///
/// Binaries are built on the host with `--features docker` (for Docker network IPs)
/// before starting Docker Compose. This avoids two race conditions:
/// 1. Other emulator tests building without --features docker concurrently
/// 2. Docker containers building concurrently from a shared volume mount
#[test]
fn test_qemu_rs_talker_listener_e2e() {
    if !require_docker_compose() {
        return;
    }
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let root = project_root();
    let compose_file = root.join("tests/qemu-baremetal/docker-compose.yml");

    assert!(
        compose_file.exists(),
        "Docker Compose file not found: {}",
        compose_file.display()
    );

    // Build binaries on host with --features docker so they use Docker network IPs
    // (192.168.100.x) instead of TAP-mode IPs (192.0.3.x).
    eprintln!("Building QEMU binaries with --features docker...");
    let talker = build_example(
        "qemu/rs-talker",
        "qemu-rs-talker",
        Some(&["docker"]),
        Some("thumbv7m-none-eabi"),
    )
    .expect("Failed to build qemu-rs-talker with docker feature");
    let listener = build_example(
        "qemu/rs-listener",
        "qemu-rs-listener",
        Some(&["docker"]),
        Some("thumbv7m-none-eabi"),
    )
    .expect("Failed to build qemu-rs-listener with docker feature");
    eprintln!("  Talker:   {}", talker.display());
    eprintln!("  Listener: {}", listener.display());

    eprintln!("Starting QEMU E2E test via Docker Compose (rs examples)...");
    eprintln!("This may take a while on first run (Docker image build).");

    // Pass host UID/GID so containers create files with correct ownership
    let host_uid = unsafe { libc::getuid() }.to_string();
    let host_gid = unsafe { libc::getgid() }.to_string();

    // Run docker compose up with QEMU_EXAMPLE=rs
    let result = duct::cmd!(
        "docker",
        "compose",
        "-f",
        compose_file.to_str().unwrap(),
        "up",
        "--build",
        "--abort-on-container-exit"
    )
    .env("QEMU_EXAMPLE", "rs")
    .env("HOST_UID", &host_uid)
    .env("HOST_GID", &host_gid)
    .dir(&root)
    .stderr_to_stdout()
    .stdout_capture()
    .unchecked()
    .run();

    // Always clean up containers
    let _ = duct::cmd!(
        "docker",
        "compose",
        "-f",
        compose_file.to_str().unwrap(),
        "down",
        "-v"
    )
    .env("HOST_UID", &host_uid)
    .env("HOST_GID", &host_gid)
    .dir(&root)
    .stderr_to_stdout()
    .stdout_capture()
    .unchecked()
    .run();

    let output = result.expect("Failed to run docker compose");
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("Docker Compose output (last 2000 chars):");
    let tail: String = stdout
        .chars()
        .rev()
        .take(2000)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    eprintln!("{}", tail);

    // Verify listener received messages
    // The listener prints "Received [N]: Hello from QEMU #M"
    let received_count = count_pattern(&stdout, "Received [");
    eprintln!("Listener received {} messages", received_count);

    assert!(
        received_count >= 3,
        "Expected listener to receive at least 3 messages, got {}.\nOutput (tail):\n{}",
        received_count,
        tail
    );

    // Verify talker published messages
    let published_count = count_pattern(&stdout, "Published:");
    eprintln!("Talker published {} messages", published_count);

    assert!(
        published_count >= 3,
        "Expected talker to publish at least 3 messages, got {}.\nOutput (tail):\n{}",
        published_count,
        tail
    );
}
