//! ESP32-C3 QEMU emulator tests
//!
//! Tests that verify ESP32-C3 examples build, boot, and communicate
//! on the Espressif QEMU fork (qemu-system-riscv32 -M esp32c3).
//!
//! Run with: `just test-qemu-esp32`
//!
//! ## Test Groups
//!
//! - **Build tests**: Verify `cargo +nightly build --release` succeeds (no QEMU needed)
//! - **Boot test**: Verify BSP banner appears on UART (QEMU needed, no networking)
//! - **Networked E2E**: Talker (tap-qemu0) → listener (tap-qemu1) via zenohd + TAP
//!
//! ## Prerequisites
//!
//! - Build tests: nightly toolchain + riscv32imc target + zenoh-pico RISC-V
//! - Boot test: + qemu-system-riscv32 (Espressif fork) + espflash
//! - Networked E2E: + TAP networking (`sudo ./scripts/qemu/setup-network.sh`) + zenohd

use nros_tests::count_pattern;
use nros_tests::esp32::*;
use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_esp32_qemu_listener, build_esp32_qemu_talker,
    build_native_listener, build_native_talker, require_zenohd,
};
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Build Tests (no QEMU needed)
// =============================================================================

/// Verify esp32-qemu-talker builds with cargo +nightly
#[test]
fn test_esp32_qemu_talker_builds() {
    if !require_riscv32_target() || !require_zenoh_pico_riscv() {
        return;
    }

    let result = build_esp32_qemu_talker();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            eprintln!("SUCCESS: esp32-qemu-talker builds at {}", binary.display());
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!("Fix with: sudo rm -rf examples/qemu-esp32/rust/zenoh/talker/target");
                eprintln!("Skipping test...");
            } else {
                panic!("esp32-qemu-talker build failed: {:?}", e);
            }
        }
    }
}

/// Verify esp32-qemu-listener builds with cargo +nightly
#[test]
fn test_esp32_qemu_listener_builds() {
    if !require_riscv32_target() || !require_zenoh_pico_riscv() {
        return;
    }

    let result = build_esp32_qemu_listener();
    match result {
        Ok(binary) => {
            assert!(
                binary.exists(),
                "Binary should exist at {}",
                binary.display()
            );
            eprintln!(
                "SUCCESS: esp32-qemu-listener builds at {}",
                binary.display()
            );
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            if err_str.contains("Permission denied") {
                eprintln!("Build failed due to permission issues (likely from Docker build)");
                eprintln!("Fix with: sudo rm -rf examples/qemu-esp32/rust/zenoh/listener/target");
                eprintln!("Skipping test...");
            } else {
                panic!("esp32-qemu-listener build failed: {:?}", e);
            }
        }
    }
}

// =============================================================================
// Boot Test (QEMU needed, no networking)
// =============================================================================

/// Verify ESP32-C3 boots and shows BSP banner on UART
#[test]
fn test_esp32_qemu_talker_boots() {
    if !require_riscv32_target() || !require_zenoh_pico_riscv() {
        return;
    }
    if !require_qemu_riscv32() || !require_espflash() {
        return;
    }

    let elf = build_esp32_qemu_talker().expect("Failed to build esp32-qemu-talker");

    // Create flash image
    let root = nros_tests::project_root();
    let flash_image = root.join("build/esp32-qemu/esp32-qemu-talker.bin");
    create_esp32_flash_image(elf, &flash_image).expect("Failed to create flash image");

    // Boot without networking
    let mut qemu =
        start_esp32_qemu(&flash_image, None, None).expect("Failed to start ESP32-C3 QEMU");

    let output = qemu
        .wait_for_output_pattern("nros ESP32-C3 QEMU BSP", Duration::from_secs(30))
        .expect("QEMU timed out waiting for BSP banner");

    assert!(
        output.contains("nros ESP32-C3 QEMU BSP"),
        "Expected BSP banner in output.\nOutput:\n{}",
        output
    );

    eprintln!("SUCCESS: ESP32-C3 QEMU boots and shows BSP banner");
}

// =============================================================================
// Networked E2E Tests (QEMU + TAP + zenohd)
// =============================================================================
//
// These tests require:
// - qemu-system-riscv32, espflash, nightly toolchain
// - TAP networking (tap-qemu0, tap-qemu1 on qemu-br bridge)
// - zenohd listening on port 7448
//
// The ESP32 firmware hardcodes tcp/192.0.3.1:7448 as the zenoh locator,
// so zenohd must listen on 0.0.0.0:7448 (fixed port).
//
// Ordering follows the Docker-based ARM QEMU pattern:
//   1. Start zenohd, verify reachable on bridge IP
//   2. Start listener (tap-qemu1), wait for subscription
//   3. Start talker (tap-qemu0), wait for publish completion
//   4. Verify listener received messages

/// Helper: build and create flash images for talker and listener
fn build_esp32_flash_images() -> (std::path::PathBuf, std::path::PathBuf) {
    let talker_elf = build_esp32_qemu_talker().expect("Failed to build esp32-qemu-talker");
    let listener_elf = build_esp32_qemu_listener().expect("Failed to build esp32-qemu-listener");

    let root = nros_tests::project_root();
    let talker_bin = root.join("build/esp32-qemu/esp32-qemu-talker.bin");
    let listener_bin = root.join("build/esp32-qemu/esp32-qemu-listener.bin");

    create_esp32_flash_image(talker_elf, &talker_bin).expect("Failed to create talker flash image");
    create_esp32_flash_image(listener_elf, &listener_bin)
        .expect("Failed to create listener flash image");

    (talker_bin, listener_bin)
}

/// Check all prerequisites for networked ESP32 tests
fn require_esp32_networked() -> bool {
    if !require_riscv32_target() {
        return false;
    }
    if !require_zenoh_pico_riscv() {
        return false;
    }
    if !require_qemu_riscv32() {
        return false;
    }
    if !require_espflash() {
        return false;
    }
    if !require_tap_network() {
        return false;
    }
    if !require_zenohd() {
        return false;
    }
    true
}

/// Test ESP32 talker (tap-qemu0) → ESP32 listener (tap-qemu1) end-to-end
///
/// Each QEMU instance uses its own TAP device for network isolation:
/// - Listener: tap-qemu1, MAC 02:00:00:00:00:02, IP 192.0.3.11
/// - Talker:   tap-qemu0, MAC 02:00:00:00:00:01, IP 192.0.3.10
///
/// Both connect to zenohd at 192.0.3.1:7448 (bridge IP).
#[test]
fn test_esp32_talker_listener_e2e() {
    if !require_esp32_networked() {
        return;
    }

    let (talker_bin, listener_bin) = build_esp32_flash_images();

    // Start zenohd on fixed port 7448 (kills any orphaned zenohd first)
    let _router = ZenohRouter::start(7448).expect("Failed to start zenohd on port 7448");

    // Verify zenohd is reachable on the bridge IP (not just localhost)
    assert!(
        wait_for_addr("192.0.3.1:7448", Duration::from_secs(10)),
        "zenohd not reachable on bridge IP 192.0.3.1:7448"
    );

    // Step 1: Start listener on tap-qemu1 (different TAP from talker)
    let mut listener =
        start_esp32_qemu(&listener_bin, Some("tap-qemu1"), Some("02:00:00:00:00:02"))
            .expect("Failed to start ESP32 listener");

    // Wait for listener to connect and subscribe
    let listener_startup = listener
        .wait_for_output_pattern("Waiting for messages...", Duration::from_secs(60))
        .expect("ESP32 listener failed to start (check zenoh connection)");

    assert!(
        listener_startup.contains("Connected!"),
        "Listener should connect to zenohd.\nOutput:\n{}",
        listener_startup
    );
    eprintln!("Listener connected and subscribed");

    // Step 2: Network stabilization — give listener time to register
    // subscription with zenohd before talker starts publishing
    std::thread::sleep(Duration::from_secs(5));

    // Step 3: Start talker on tap-qemu0 (different TAP from listener)
    let mut talker = start_esp32_qemu(&talker_bin, Some("tap-qemu0"), Some("02:00:00:00:00:01"))
        .expect("Failed to start ESP32 talker");

    // Wait for talker to finish publishing
    let talker_output = talker
        .wait_for_output_pattern("Done publishing 5 messages.", Duration::from_secs(60))
        .expect("ESP32 talker timed out waiting for publish completion");

    assert!(
        talker_output.contains("Connected!"),
        "Talker should connect to zenohd.\nOutput:\n{}",
        talker_output
    );

    let published_count = count_pattern(&talker_output, "Published:");
    eprintln!("Talker published {} messages", published_count);

    // Step 4: Wait for listener to receive messages
    let listener_output = listener
        .wait_for_output_pattern("Received [", Duration::from_secs(30))
        .unwrap_or_default();

    let all_output = format!("{}{}", listener_startup, listener_output);
    let received_count = count_pattern(&all_output, "Received [");
    eprintln!("Listener received {} messages", received_count);

    assert!(
        received_count >= 1,
        "Expected listener to receive at least 1 message, got {}.\nOutput:\n{}",
        received_count,
        all_output
    );

    eprintln!("SUCCESS: ESP32 talker → ESP32 listener E2E works");
}

// =============================================================================
// Cross-Architecture Interop Tests (ESP32 QEMU ↔ native)
// =============================================================================
//
// These tests verify that ESP32 QEMU examples can communicate with native
// nros examples via CDR-encoded Int32 on the /chatter ROS 2 topic.
//
// Both ESP32 and native processes connect to the same zenohd on port 7448:
// - ESP32 via TAP bridge at 192.0.3.1:7448
// - Native via localhost at 127.0.0.1:7448

/// Helper: build ESP32 talker flash image only
fn build_esp32_talker_flash() -> std::path::PathBuf {
    let talker_elf = build_esp32_qemu_talker().expect("Failed to build esp32-qemu-talker");
    let root = nros_tests::project_root();
    let talker_bin = root.join("build/esp32-qemu/esp32-qemu-talker.bin");
    create_esp32_flash_image(talker_elf, &talker_bin).expect("Failed to create talker flash image");
    talker_bin
}

/// Helper: build ESP32 listener flash image only
fn build_esp32_listener_flash() -> std::path::PathBuf {
    let listener_elf = build_esp32_qemu_listener().expect("Failed to build esp32-qemu-listener");
    let root = nros_tests::project_root();
    let listener_bin = root.join("build/esp32-qemu/esp32-qemu-listener.bin");
    create_esp32_flash_image(listener_elf, &listener_bin)
        .expect("Failed to create listener flash image");
    listener_bin
}

/// Test ESP32 talker → native listener cross-architecture interop
///
/// ESP32 publishes CDR Int32 on /chatter via TAP bridge,
/// native listener receives on localhost.
#[test]
fn test_esp32_to_native() {
    if !require_esp32_networked() {
        return;
    }

    // Only need talker flash + native listener
    let talker_bin = build_esp32_talker_flash();
    let native_listener = build_native_listener().expect("Failed to build native listener");

    // Start zenohd on fixed port 7448 (kills any orphaned zenohd first)
    let _router = ZenohRouter::start(7448).expect("Failed to start zenohd on port 7448");

    // Verify zenohd is reachable on the bridge IP
    assert!(
        wait_for_addr("192.0.3.1:7448", Duration::from_secs(10)),
        "zenohd not reachable on bridge IP 192.0.3.1:7448"
    );

    // Start native listener on localhost (connects to same zenohd)
    let mut listener_cmd = Command::new(native_listener);
    listener_cmd
        .env("ZENOH_LOCATOR", "tcp/127.0.0.1:7448")
        .env("RUST_LOG", "info");
    let mut native_proc = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start native listener");

    // Wait for native listener to be ready
    let _ = native_proc.wait_for_output_pattern("Waiting for", Duration::from_secs(10));

    // Stabilization delay
    std::thread::sleep(Duration::from_secs(5));

    // Start ESP32 talker on tap-qemu0
    let mut talker = start_esp32_qemu(&talker_bin, Some("tap-qemu0"), Some("02:00:00:00:00:01"))
        .expect("Failed to start ESP32 talker");

    // Wait for ESP32 talker to finish publishing
    let talker_output = talker
        .wait_for_output_pattern("Done publishing 5 messages.", Duration::from_secs(60))
        .expect("ESP32 talker timed out");

    assert!(
        talker_output.contains("Connected!"),
        "ESP32 talker should connect to zenohd.\nOutput:\n{}",
        talker_output
    );

    // Wait for native listener to receive messages
    let listener_output = native_proc
        .wait_for_output_pattern("Received:", Duration::from_secs(15))
        .unwrap_or_default();

    let received_count = count_pattern(&listener_output, "Received:");
    eprintln!(
        "Native listener received {} messages from ESP32 talker",
        received_count
    );

    assert!(
        received_count >= 1,
        "Expected native listener to receive at least 1 message from ESP32, got {}.\nListener output:\n{}",
        received_count,
        listener_output
    );

    eprintln!("SUCCESS: ESP32 talker → native listener interop works");
}

/// Test native talker → ESP32 listener cross-architecture interop
///
/// Native publishes CDR Int32 on /chatter via localhost,
/// ESP32 listener receives on TAP bridge.
#[test]
fn test_native_to_esp32() {
    if !require_esp32_networked() {
        return;
    }

    // Only need listener flash + native talker
    let listener_bin = build_esp32_listener_flash();
    let native_talker = build_native_talker().expect("Failed to build native talker");

    // Start zenohd on fixed port 7448 (kills any orphaned zenohd first)
    let _router = ZenohRouter::start(7448).expect("Failed to start zenohd on port 7448");

    // Verify zenohd is reachable on the bridge IP
    assert!(
        wait_for_addr("192.0.3.1:7448", Duration::from_secs(10)),
        "zenohd not reachable on bridge IP 192.0.3.1:7448"
    );

    // Start ESP32 listener on tap-qemu1
    let mut esp32_listener =
        start_esp32_qemu(&listener_bin, Some("tap-qemu1"), Some("02:00:00:00:00:02"))
            .expect("Failed to start ESP32 listener");

    // Wait for ESP32 listener to connect and subscribe
    let listener_startup = esp32_listener
        .wait_for_output_pattern("Waiting for messages...", Duration::from_secs(60))
        .expect("ESP32 listener failed to start");

    assert!(
        listener_startup.contains("Connected!"),
        "ESP32 listener should connect to zenohd.\nOutput:\n{}",
        listener_startup
    );

    // Stabilization delay
    std::thread::sleep(Duration::from_secs(5));

    // Start native talker on localhost (publishes every 1s)
    let mut talker_cmd = Command::new(native_talker);
    talker_cmd
        .env("ZENOH_LOCATOR", "tcp/127.0.0.1:7448")
        .env("RUST_LOG", "info");
    let mut native_proc = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start native talker");

    // Wait for native talker to start publishing
    let _ = native_proc.wait_for_output_pattern("Published:", Duration::from_secs(10));

    // Wait for ESP32 listener to receive messages
    let listener_output = esp32_listener
        .wait_for_output_pattern("Received [", Duration::from_secs(30))
        .unwrap_or_default();

    let all_output = format!("{}{}", listener_startup, listener_output);
    let received_count = count_pattern(&all_output, "Received [");
    eprintln!(
        "ESP32 listener received {} messages from native talker",
        received_count
    );

    assert!(
        received_count >= 1,
        "Expected ESP32 listener to receive at least 1 message from native, got {}.\nESP32 output:\n{}",
        received_count,
        all_output
    );

    eprintln!("SUCCESS: native talker → ESP32 listener interop works");
}

// =============================================================================
// Detection Tests (always run)
// =============================================================================

#[test]
fn test_esp32_qemu_riscv32_detection() {
    let available = is_qemu_riscv32_available();
    eprintln!("qemu-system-riscv32 available: {}", available);
}

#[test]
fn test_esp32_riscv32_target_detection() {
    let available = is_riscv32_target_available();
    eprintln!(
        "riscv32imc-unknown-none-elf target available: {}",
        available
    );
}

#[test]
fn test_esp32_espflash_detection() {
    let available = is_espflash_available();
    eprintln!("espflash available: {}", available);
}
