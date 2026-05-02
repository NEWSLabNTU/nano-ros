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
    freertos::{
        build_freertos_action_client, build_freertos_action_server, build_freertos_listener,
        build_freertos_service_client, build_freertos_service_server, build_freertos_talker,
        is_arm_gcc_available, is_freertos_available, is_lwip_available,
    },
    is_qemu_available, is_zenohd_available,
};

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
// Build tests (require FREERTOS_DIR + LWIP_DIR + arm-none-eabi-gcc)
// =============================================================================

#[test]
fn test_freertos_all_examples_build() {
    if !require_freertos() {
        nros_tests::skip!("require_freertos check failed");
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
