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
    QemuProcess, SocatPtyPair, ZenohRouter, build_qemu_bsp_listener, build_qemu_bsp_talker,
    build_qemu_lan9118, build_qemu_rtic_action_client, build_qemu_rtic_action_server,
    build_qemu_rtic_listener, build_qemu_rtic_mixed_listener, build_qemu_rtic_mixed_talker,
    build_qemu_rtic_service_client, build_qemu_rtic_service_server, build_qemu_rtic_talker,
    build_qemu_serial_listener, build_qemu_serial_talker, build_qemu_wcet_bench,
    is_arm_toolchain_available, is_qemu_available, is_socat_available, parse_test_results,
    qemu_binary, require_zenoh_pico_arm,
};
use nros_tests::platform;
use nros_tests::{assert_output_contains, assert_output_excludes, count_pattern, wait_for_port};
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
// BSP Network Tests (Require Docker or slirp networking)
// =============================================================================
//
// The BSP examples use the MPS2-AN385 machine with LAN9118 Ethernet.
// QEMU uses slirp (user-mode) networking: each instance gets an isolated
// 10.0.2.0/24 network. Firmware connects to zenohd via slirp gateway
// 10.0.2.2:<port>, which maps to host 127.0.0.1:<port>. No TAP bridge needed.
//
// To run BSP network tests:
//   just test-rust-qemu-baremetal-bsp  (uses Docker)
//
// Or manually:
//   zenohd --listen tcp/0.0.0.0:7447
//   ./scripts/qemu/launch-mps2-an385.sh --binary <path>

/// Test that qemu-bsp-talker starts (requires Docker or QEMU with slirp networking)
///
/// NOTE: This test is skipped by default as it requires network setup.
/// Use `just test-rust-qemu-baremetal-bsp` for the full Docker-based test.
#[test]
fn test_qemu_bsp_talker_starts() {
    // BSP examples require MPS2-AN385 with networking, which isn't available
    // in the standard test environment. The Docker-based test handles this.
    eprintln!("Skipping test: BSP start tests require Docker or QEMU networking");
    eprintln!("Run with: just test-rust-qemu-baremetal-bsp");
    println!("INFO: BSP network tests skipped (use Docker for full test)");
}

/// Test that qemu-bsp-listener starts (requires Docker or QEMU with slirp networking)
///
/// NOTE: This test is skipped by default as it requires network setup.
/// Use `just test-rust-qemu-baremetal-bsp` for the full Docker-based test.
#[test]
fn test_qemu_bsp_listener_starts() {
    // BSP examples require MPS2-AN385 with networking, which isn't available
    // in the standard test environment. The Docker-based test handles this.
    eprintln!("Skipping test: BSP start tests require Docker or QEMU networking");
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

// =============================================================================
// Serial QEMU Build Tests (MPS2-AN385 + CMSDK UART)
// =============================================================================

/// Test that qemu-serial-talker builds successfully
#[test]
fn test_qemu_serial_talker_builds() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let result = build_qemu_serial_talker();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!("SUCCESS: qemu-serial-talker builds at {}", binary.display());
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!(
                    "Fix with: sudo rm -rf examples/qemu-arm-baremetal/rust/zenoh/serial-talker/target"
                );
                eprintln!("Skipping test...");
            } else {
                panic!("qemu-serial-talker build failed: {:?}", e);
            }
        }
    }
}

/// Test that qemu-serial-listener builds successfully
#[test]
fn test_qemu_serial_listener_builds() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    let result = build_qemu_serial_listener();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            println!(
                "SUCCESS: qemu-serial-listener builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!(
                    "Fix with: sudo rm -rf examples/qemu-arm-baremetal/rust/zenoh/serial-listener/target"
                );
                eprintln!("Skipping test...");
            } else {
                panic!("qemu-serial-listener build failed: {:?}", e);
            }
        }
    }
}

// =============================================================================
// Serial QEMU E2E Tests (MPS2-AN385 + CMSDK UART + zenohd serial)
// =============================================================================

/// Test serial pub/sub between two QEMU instances via zenohd serial bridge.
///
/// Architecture:
///   socat pair A:  QEMU listener UART0 ↔ zenohd serial listener
///   socat pair B:  QEMU talker  UART0 ↔ zenohd serial listener
///
/// Using socat PTY pairs ensures both ends exist before either side starts,
/// avoiding the race where firmware sends InitSyn before zenohd is ready.
/// `-display none -monitor none` avoids `-nographic`'s implicit `-serial
/// mon:stdio` which hijacks UART0 for the QEMU monitor.
#[test]
fn test_qemu_serial_pubsub_e2e() {
    require_arm_toolchain();
    require_qemu();
    if !require_zenoh_pico_arm() {
        return;
    }
    if !is_socat_available() {
        eprintln!("Skipping test: socat not found");
        return;
    }

    // Build both binaries
    let talker_bin = build_qemu_serial_talker().expect("Failed to build serial-talker");
    let listener_bin = build_qemu_serial_listener().expect("Failed to build serial-listener");

    // Create socat PTY pairs: one for listener, one for talker.
    // Each pair links QEMU's UART0 to zenohd's serial listener.
    let tmp_dir = nros_tests::project_root().join("tmp");
    std::fs::create_dir_all(&tmp_dir).expect("Failed to create tmp dir");

    let listener_pair = SocatPtyPair::create(
        tmp_dir.join("serial-listener-qemu").to_str().unwrap(),
        tmp_dir.join("serial-listener-zenohd").to_str().unwrap(),
    )
    .expect("Failed to create listener socat PTY pair");
    eprintln!(
        "Listener PTY pair: {} ↔ {}",
        listener_pair.qemu_path, listener_pair.zenohd_path
    );

    let talker_pair = SocatPtyPair::create(
        tmp_dir.join("serial-talker-qemu").to_str().unwrap(),
        tmp_dir.join("serial-talker-zenohd").to_str().unwrap(),
    )
    .expect("Failed to create talker socat PTY pair");
    eprintln!(
        "Talker PTY pair: {} ↔ {}",
        talker_pair.qemu_path, talker_pair.zenohd_path
    );

    // Start zenohd with serial listeners on the zenohd side of each pair.
    // zenohd must be ready before QEMU starts, so the InitSyn handshake succeeds.
    eprintln!("Starting zenohd with serial listeners...");
    let _zenohd =
        ZenohRouter::start_serial(&[&listener_pair.zenohd_path, &talker_pair.zenohd_path])
            .expect("Failed to start zenohd with serial listeners");

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting serial listener QEMU...");
    let mut listener =
        QemuProcess::start_mps2_an385_with_serial(&listener_bin, &listener_pair.qemu_path)
            .expect("Failed to start listener QEMU");

    // Brief delay for listener to subscribe before talker starts publishing
    std::thread::sleep(Duration::from_secs(5));

    // Start talker QEMU
    eprintln!("Starting serial talker QEMU...");
    let mut talker = QemuProcess::start_mps2_an385_with_serial(&talker_bin, &talker_pair.qemu_path)
        .expect("Failed to start talker QEMU");

    // Wait for listener to receive messages (examples now run forever)
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to publish messages
    let talker_output = talker
        .wait_for_output_pattern("Published:", Duration::from_secs(30))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify communication
    let received = count_pattern(&listener_output, "Received:");
    let published = count_pattern(&talker_output, "Published:");
    eprintln!(
        "Serial QEMU pubsub: published={}, received={}",
        published, received
    );

    assert!(received > 0, "Serial listener received 0 messages");
    assert!(published > 0, "Serial talker published 0 messages");
}

// =============================================================================
// RTIC QEMU Build Tests (MPS2-AN385)
// =============================================================================

#[test]
fn test_qemu_rtic_talker_builds() {
    require_arm_toolchain();

    let binary = build_qemu_rtic_talker().expect("Failed to build qemu-rtic-talker");
    println!("SUCCESS: qemu-rtic-talker builds at {}", binary.display());
}

#[test]
fn test_qemu_rtic_listener_builds() {
    require_arm_toolchain();

    let binary = build_qemu_rtic_listener().expect("Failed to build qemu-rtic-listener");
    println!("SUCCESS: qemu-rtic-listener builds at {}", binary.display());
}

// =============================================================================
// RTIC QEMU Networked Tests (MPS2-AN385 + LAN9118 + zenohd)
// =============================================================================

#[test]
fn test_qemu_rtic_pubsub_e2e() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    // Build both binaries
    let talker_bin = build_qemu_rtic_talker().expect("Failed to build rtic-talker");
    let listener_bin = build_qemu_rtic_listener().expect("Failed to build rtic-listener");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd =
        ZenohRouter::start(platform::BAREMETAL.zenohd_port).expect("Failed to start zenohd");

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(platform::BAREMETAL.zenohd_port, Duration::from_secs(5)),
        "zenohd not reachable on platform port"
    );

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting RTIC listener QEMU...");
    let mut listener = QemuProcess::start_mps2_an385_networked(listener_bin)
        .expect("Failed to start listener QEMU");

    // Stabilization delay: bare-metal boot + smoltcp init + zenoh connect
    std::thread::sleep(Duration::from_secs(8));

    // Start talker QEMU
    eprintln!("Starting RTIC talker QEMU...");
    let mut talker =
        QemuProcess::start_mps2_an385_networked(talker_bin).expect("Failed to start talker QEMU");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to publish messages
    let talker_output = talker
        .wait_for_output_pattern("Published:", Duration::from_secs(30))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify communication
    let received = count_pattern(&listener_output, "Received:");
    let published = count_pattern(&talker_output, "Published:");
    eprintln!(
        "RTIC QEMU pubsub: published={}, received={}",
        published, received
    );

    assert!(received > 0, "RTIC QEMU listener received 0 messages");
    assert!(published > 0, "RTIC QEMU talker published 0 messages");
}

// =============================================================================
// RTIC QEMU Service/Action Build Tests (MPS2-AN385)
// =============================================================================

#[test]
fn test_qemu_rtic_service_server_builds() {
    require_arm_toolchain();

    let binary =
        build_qemu_rtic_service_server().expect("Failed to build qemu-rtic-service-server");
    println!(
        "SUCCESS: qemu-rtic-service-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_qemu_rtic_service_client_builds() {
    require_arm_toolchain();

    let binary =
        build_qemu_rtic_service_client().expect("Failed to build qemu-rtic-service-client");
    println!(
        "SUCCESS: qemu-rtic-service-client builds at {}",
        binary.display()
    );
}

#[test]
fn test_qemu_rtic_action_server_builds() {
    require_arm_toolchain();

    let binary = build_qemu_rtic_action_server().expect("Failed to build qemu-rtic-action-server");
    println!(
        "SUCCESS: qemu-rtic-action-server builds at {}",
        binary.display()
    );
}

#[test]
fn test_qemu_rtic_action_client_builds() {
    require_arm_toolchain();

    let binary = build_qemu_rtic_action_client().expect("Failed to build qemu-rtic-action-client");
    println!(
        "SUCCESS: qemu-rtic-action-client builds at {}",
        binary.display()
    );
}

// =============================================================================
// RTIC QEMU Service/Action Networked Tests (MPS2-AN385 + LAN9118 + zenohd)
// =============================================================================

/// Service E2E test for RTIC on QEMU.
///
/// Tests 4 service calls (AddTwoInts) between server and client QEMU instances.
#[test]
fn test_qemu_rtic_service_e2e() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    // Build both binaries
    let server_bin = build_qemu_rtic_service_server().expect("Failed to build rtic-service-server");
    let client_bin = build_qemu_rtic_service_client().expect("Failed to build rtic-service-client");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd =
        ZenohRouter::start(platform::BAREMETAL.zenohd_port).expect("Failed to start zenohd");

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(platform::BAREMETAL.zenohd_port, Duration::from_secs(5)),
        "zenohd not reachable on platform port"
    );

    // Start server QEMU first
    eprintln!("Starting RTIC service server QEMU...");
    let mut server =
        QemuProcess::start_mps2_an385_networked(server_bin).expect("Failed to start server QEMU");

    // Stabilization delay: bare-metal boot + smoltcp init + zenoh connect + queryable discovery
    // Services need longer than pub/sub because zenoh queryable discovery takes time
    std::thread::sleep(Duration::from_secs(8));

    // Start client QEMU
    eprintln!("Starting RTIC service client QEMU...");
    let mut client =
        QemuProcess::start_mps2_an385_networked(client_bin).expect("Failed to start client QEMU");

    // Wait for client to complete (it exits after 4 service calls)
    let client_output = client
        .wait_for_output(Duration::from_secs(90))
        .unwrap_or_default();

    // Collect server output
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    client.kill();
    server.kill();

    eprintln!("Server output:\n{}", server_output);
    eprintln!("Client output:\n{}", client_output);

    // Verify service communication
    assert!(
        client_output.contains("All service calls completed"),
        "RTIC QEMU service client did not complete all calls"
    );

    let handled = count_pattern(&server_output, "Handled:");
    eprintln!("RTIC QEMU service: server handled {} requests", handled);
    assert!(
        handled >= 1,
        "RTIC QEMU service server did not handle any requests (got {})",
        handled
    );
}

/// Action E2E test for RTIC on QEMU.
///
/// Tests Fibonacci action (goal, feedback, result) between server and client QEMU instances.
#[test]
fn test_qemu_rtic_action_e2e() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    // Build both binaries
    let server_bin = build_qemu_rtic_action_server().expect("Failed to build rtic-action-server");
    let client_bin = build_qemu_rtic_action_client().expect("Failed to build rtic-action-client");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd =
        ZenohRouter::start(platform::BAREMETAL.zenohd_port).expect("Failed to start zenohd");

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(platform::BAREMETAL.zenohd_port, Duration::from_secs(5)),
        "zenohd not reachable on platform port"
    );

    // Start server QEMU first
    eprintln!("Starting RTIC action server QEMU...");
    let mut server =
        QemuProcess::start_mps2_an385_networked(server_bin).expect("Failed to start server QEMU");

    // Stabilization delay: bare-metal boot + smoltcp init + zenoh connect
    std::thread::sleep(Duration::from_secs(8));

    // Start client QEMU
    eprintln!("Starting RTIC action client QEMU...");
    let mut client =
        QemuProcess::start_mps2_an385_networked(client_bin).expect("Failed to start client QEMU");

    // Wait for client to complete (it exits after receiving feedback)
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    // Collect server output
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    client.kill();
    server.kill();

    eprintln!("Server output:\n{}", server_output);
    eprintln!("Client output:\n{}", client_output);

    // Verify action communication
    assert!(
        client_output.contains("Goal accepted"),
        "RTIC QEMU action client: goal was not accepted"
    );
    assert!(
        client_output.contains("Got") && client_output.contains("feedback messages"),
        "RTIC QEMU action client did not receive feedback messages"
    );
    assert!(
        server_output.contains("Goal accepted"),
        "RTIC QEMU action server did not accept goal"
    );
}

// =============================================================================
// RTIC Mixed-Priority QEMU Build Tests (MPS2-AN385, ffi-sync)
// =============================================================================

#[test]
fn test_qemu_rtic_mixed_talker_builds() {
    require_arm_toolchain();

    let binary = build_qemu_rtic_mixed_talker().expect("Failed to build qemu-rtic-mixed-talker");
    println!(
        "SUCCESS: qemu-rtic-mixed-talker builds at {}",
        binary.display()
    );
}

#[test]
fn test_qemu_rtic_mixed_listener_builds() {
    require_arm_toolchain();

    let binary =
        build_qemu_rtic_mixed_listener().expect("Failed to build qemu-rtic-mixed-listener");
    println!(
        "SUCCESS: qemu-rtic-mixed-listener builds at {}",
        binary.display()
    );
}

// =============================================================================
// RTIC Mixed-Priority QEMU Networked Test (MPS2-AN385 + LAN9118 + zenohd)
// =============================================================================

/// Mixed-priority pubsub E2E test for RTIC on QEMU.
///
/// Same as `test_qemu_rtic_pubsub_e2e` but with `publish`/`listen` at priority 2
/// and `net_poll` at priority 1. The `ffi-sync` feature prevents FFI state
/// corruption when the higher-priority task preempts `spin_once(0)`.
#[test]
fn test_qemu_rtic_mixed_priority_pubsub_e2e() {
    require_arm_toolchain();
    if !require_zenoh_pico_arm() {
        return;
    }

    // Build both binaries
    let talker_bin = build_qemu_rtic_mixed_talker().expect("Failed to build rtic-mixed-talker");
    let listener_bin =
        build_qemu_rtic_mixed_listener().expect("Failed to build rtic-mixed-listener");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd =
        ZenohRouter::start(platform::BAREMETAL.zenohd_port).expect("Failed to start zenohd");

    // Verify zenohd is reachable on localhost (slirp gateway forwards to host)
    assert!(
        wait_for_port(platform::BAREMETAL.zenohd_port, Duration::from_secs(5)),
        "zenohd not reachable on platform port"
    );

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting RTIC mixed-priority listener QEMU...");
    let mut listener = QemuProcess::start_mps2_an385_networked(listener_bin)
        .expect("Failed to start listener QEMU");

    // Stabilization delay: bare-metal boot + smoltcp init + zenoh connect
    std::thread::sleep(Duration::from_secs(8));

    // Start talker QEMU
    eprintln!("Starting RTIC mixed-priority talker QEMU...");
    let mut talker =
        QemuProcess::start_mps2_an385_networked(talker_bin).expect("Failed to start talker QEMU");

    // Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to publish messages
    let talker_output = talker
        .wait_for_output_pattern("Published:", Duration::from_secs(30))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify communication
    let received = count_pattern(&listener_output, "Received:");
    let published = count_pattern(&talker_output, "Published:");
    eprintln!(
        "RTIC mixed-priority QEMU pubsub: published={}, received={}",
        published, received
    );

    assert!(
        received > 0,
        "RTIC mixed-priority QEMU listener received 0 messages"
    );
    assert!(
        published > 0,
        "RTIC mixed-priority QEMU talker published 0 messages"
    );
}
