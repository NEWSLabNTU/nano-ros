//! FreeRTOS QEMU MPS2-AN385 integration tests
//!
//! Tests that verify FreeRTOS examples build and run on QEMU MPS2-AN385 (Cortex-M3).
//! FreeRTOS examples use `thumbv7m-none-eabi` target with `no_std` + lwIP networking.
//!
//! Prerequisites:
//! - `FREERTOS_DIR` env var pointing to FreeRTOS kernel source (e.g., `third-party/freertos/kernel`)
//! - `LWIP_DIR` env var pointing to lwIP source (e.g., `third-party/freertos/lwip`)
//! - `arm-none-eabi-gcc` toolchain installed
//! - `qemu-system-arm` with MPS2-AN385 machine support
//!
//! The E2E test bodies live in `tests/rtos_e2e.rs` (parametrised over
//! platform × language × variant). This file keeps the prerequisite
//! detection test and the all-examples-build smoke test so FreeRTOS
//! diagnostics remain greppable under `--test freertos_qemu`.
//!
//! Run with: `just test-freertos`
//! Or: `cargo nextest run -p nros-tests --test freertos_qemu`

use nros_tests::fixtures::{
    QemuProcess, Rmw, build_freertos_rust_example_rmw,
    freertos::{is_arm_gcc_available, is_freertos_available, is_lwip_available},
    is_qemu_available, is_zenohd_available,
};
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

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
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_freertos_detection() {
    let freertos = is_freertos_available();
    let lwip = is_lwip_available();
    let arm_gcc = is_arm_gcc_available();
    let qemu = is_qemu_available();
    let zenohd = is_zenohd_available();
    eprintln!("FreeRTOS available: {}", freertos);
    eprintln!("lwIP available: {}", lwip);
    eprintln!("arm-none-eabi-gcc available: {}", arm_gcc);
    eprintln!("QEMU available: {}", qemu);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// (Phase 182.3) `test_freertos_all_examples_build` removed — it rebuilt every
// FreeRTOS example, exactly what `build-all` / `build-test-fixtures` does
// before `test-all` (the `_require-fixtures` preflight gates on it). The
// per-role binaries are consumed by the `rtos_e2e` Platform__Freertos tests.
// =============================================================================

#[test]
fn test_freertos_rust_talker_cyclonedds_boot() {
    if !require_freertos() {
        nros_tests::skip!("require_freertos check failed");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let path = build_freertos_rust_example_rmw(
        "talker",
        "freertos_rust_talker_cyclonedds",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!(
            "qemu-arm-freertos/rust/talker cyclonedds not prebuilt; run \
             `just freertos build-fixtures` first: {:?}",
            e
        )
    });

    let mut qemu = QemuProcess::start_mps2_an385_networked(&path)
        .expect("spawn FreeRTOS Rust CycloneDDS talker");
    let output = qemu
        .wait_for_output_pattern("Published:", Duration::from_secs(90))
        .unwrap_or_default();
    qemu.kill();

    eprintln!("FreeRTOS Rust CycloneDDS talker output:\n{}", output);

    assert!(
        output.contains("Publisher declared") || output.contains("Publishing messages"),
        "CycloneDDS talker did not reach publisher startup.\nOutput:\n{}",
        output
    );
    assert!(
        output.contains("Published:"),
        "CycloneDDS talker did not publish.\nOutput:\n{}",
        output
    );
}

#[test]
fn test_freertos_rust_cyclonedds_local_pubsub_e2e() {
    if !require_freertos() {
        nros_tests::skip!("require_freertos check failed");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let talker_path = build_freertos_rust_example_rmw(
        "talker",
        "freertos_rust_talker_cyclonedds",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!(
            "qemu-arm-freertos/rust/talker cyclonedds not prebuilt; run \
             `just freertos build-fixtures` first: {:?}",
            e
        )
    });

    let mut qemu = QemuProcess::start_mps2_an385_networked(&talker_path)
        .expect("spawn FreeRTOS Rust CycloneDDS local pubsub fixture");
    let output = qemu
        .wait_for_output_pattern("Loopback received:", Duration::from_secs(90))
        .unwrap_or_default();
    qemu.kill();

    eprintln!("FreeRTOS Rust CycloneDDS local pubsub output:\n{}", output);
    assert!(
        output.contains("Published:"),
        "CycloneDDS talker did not publish.\nOutput:\n{}",
        output
    );
    assert!(
        output.contains("Loopback received:"),
        "CycloneDDS local subscriber did not receive a sample.\nOutput:\n{}",
        output
    );
}
