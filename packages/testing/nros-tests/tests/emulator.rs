//! Emulator tests for nros
//!
//! Tests that run on QEMU Cortex-M3 emulator without physical hardware.
//! These verify CDR serialization, Node API, and type metadata work on embedded targets.
//!
//! Run with: `cargo test -p nano-ros-tests --test emulator -- --nocapture`
//! Or: `just test-rust-emulator`
//!
//! ## BSP Tests
//!
//! The BSP (Board Support Package) tests verify the simplified nros-mps2-an385 API:
//! - `just test-qemu-bsp` - Run BSP build and startup tests

use nros_tests::fixtures::{
    QemuProcess, build_qemu_bsp_listener, build_qemu_bsp_talker, build_qemu_lan9118,
    build_qemu_wcet_bench, is_arm_toolchain_available, is_qemu_available, parse_test_results,
    qemu_binary, require_zenoh_pico_arm,
};
use nros_tests::{assert_output_contains, assert_output_excludes, count_pattern};
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
// QEMU WCET Benchmark
// =============================================================================

#[test]
fn test_qemu_wcet_benchmark() {
    if !is_qemu_available() || !is_arm_toolchain_available() {
        eprintln!("Skipping test: qemu-system-arm or ARM toolchain not available");
        return;
    }

    let binary = build_qemu_wcet_bench().expect("Failed to build qemu-wcet-bench");

    let mut qemu = QemuProcess::start_cortex_m3(binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(60))
        .expect("QEMU timed out");

    // Note: [PASS] is printed after "Benchmark complete" but wait_for_output
    // kills the process on that marker, so we may not capture [PASS].
    assert_output_contains(&output, &["Benchmark complete"]);
    assert_output_excludes(&output, &["[FAIL]"]);
}

// =============================================================================
// QEMU LAN9118 Driver Test
// =============================================================================

#[test]
fn test_qemu_lan9118_driver() {
    if !is_qemu_available() || !is_arm_toolchain_available() {
        eprintln!("Skipping test: qemu-system-arm or ARM toolchain not available");
        return;
    }

    let binary = build_qemu_lan9118().expect("Failed to build qemu-lan9118");

    let mut qemu = QemuProcess::start_mps2_an385(binary).expect("Failed to start QEMU");

    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out");

    let (passed, failed) = parse_test_results(&output);

    assert!(
        passed >= 5,
        "Expected at least 5 tests to pass, got {}. Output:\n{}",
        passed,
        output
    );
    assert_eq!(
        failed, 0,
        "Expected no failures, got {}. Output:\n{}",
        failed, output
    );
    assert_output_contains(&output, &["All tests passed"]);
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
// Tests for the simplified nros-mps2-an385 API (Board Support Package).
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
                eprintln!(
                    "Fix with: sudo rm -rf examples/qemu-arm-baremetal/rust/zenoh/talker/target"
                );
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
                eprintln!(
                    "Fix with: sudo rm -rf examples/qemu-arm-baremetal/rust/zenoh/listener/target"
                );
                eprintln!("Skipping test...");
            } else {
                panic!("qemu-bsp-listener build failed: {:?}", e);
            }
        }
    }
}

// =============================================================================
// RTIC Build Tests (STM32F4, cross-compiled only — no QEMU for this board)
// =============================================================================

/// Check if thumbv7em-none-eabihf target is installed (STM32F4/Cortex-M4F)
fn require_arm_m4_toolchain() {
    if !std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("thumbv7em-none-eabihf"))
        .unwrap_or(false)
    {
        eprintln!("Skipping test: thumbv7em-none-eabihf target not installed");
        return;
    }
}

/// Test that stm32f4-rtic-talker builds successfully.
/// Unlike QEMU BSP tests, STM32F4 examples build zenoh-pico from source
/// during cargo build (via zpico-sys build script) — no pre-built library needed.
#[test]
fn test_rtic_talker_builds() {
    require_arm_m4_toolchain();

    let result = nros_tests::fixtures::build_rtic_talker();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!(
                "SUCCESS: stm32f4-rtic-talker builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            panic!("stm32f4-rtic-talker build failed: {:?}", e);
        }
    }
}

/// Test that stm32f4-rtic-listener builds successfully.
#[test]
fn test_rtic_listener_builds() {
    require_arm_m4_toolchain();

    let result = nros_tests::fixtures::build_rtic_listener();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!(
                "SUCCESS: stm32f4-rtic-listener builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            panic!("stm32f4-rtic-listener build failed: {:?}", e);
        }
    }
}

/// Test that stm32f4-rtic-service-server builds successfully.
#[test]
fn test_rtic_service_server_builds() {
    require_arm_m4_toolchain();

    let result = nros_tests::fixtures::build_rtic_service_server();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!(
                "SUCCESS: stm32f4-rtic-service-server builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            panic!("stm32f4-rtic-service-server build failed: {:?}", e);
        }
    }
}

/// Test that stm32f4-rtic-service-client builds successfully.
#[test]
fn test_rtic_service_client_builds() {
    require_arm_m4_toolchain();

    let result = nros_tests::fixtures::build_rtic_service_client();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!(
                "SUCCESS: stm32f4-rtic-service-client builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            panic!("stm32f4-rtic-service-client build failed: {:?}", e);
        }
    }
}

/// Test that stm32f4-rtic-action-server builds successfully.
#[test]
fn test_rtic_action_server_builds() {
    require_arm_m4_toolchain();

    let result = nros_tests::fixtures::build_rtic_action_server();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!(
                "SUCCESS: stm32f4-rtic-action-server builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            panic!("stm32f4-rtic-action-server build failed: {:?}", e);
        }
    }
}

/// Test that stm32f4-rtic-action-client builds successfully.
#[test]
fn test_rtic_action_client_builds() {
    require_arm_m4_toolchain();

    let result = nros_tests::fixtures::build_rtic_action_client();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!(
                "SUCCESS: stm32f4-rtic-action-client builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            panic!("stm32f4-rtic-action-client build failed: {:?}", e);
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
            eprintln!("Fix with: sudo rm -rf examples/qemu-arm-baremetal/rust/zenoh/talker/target");
            eprintln!("Skipping test...");
        }
        (_, Err(e)) if has_perm_error(e) => {
            eprintln!("Listener build failed due to permission issues");
            eprintln!(
                "Fix with: sudo rm -rf examples/qemu-arm-baremetal/rust/zenoh/listener/target"
            );
            eprintln!("Skipping test...");
        }
        (Err(e), _) => panic!("BSP talker build failed: {:?}", e),
        (_, Err(e)) => panic!("BSP listener build failed: {:?}", e),
    }
}
