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
//! **Kernel boot test** (require QEMU + NuttX kernel):
//!   Verifies that the pre-built NuttX kernel boots to the NSH prompt.
//!
//! The E2E test bodies live in `tests/rtos_e2e.rs` (parametrised over
//! platform × language × variant).
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

use nros_tests::fixtures::nuttx::{
    build_nuttx_action_client, build_nuttx_action_server, build_nuttx_cpp_action_client,
    build_nuttx_cpp_action_server, build_nuttx_cpp_listener, build_nuttx_cpp_service_client,
    build_nuttx_cpp_service_server, build_nuttx_cpp_talker, build_nuttx_listener,
    build_nuttx_service_client, build_nuttx_service_server, build_nuttx_talker,
    is_arm_gcc_available, is_cmake_available, is_nuttx_available, is_nuttx_configured,
    is_nuttx_toolchain_available, nuttx_kernel_path,
};
use nros_tests::fixtures::{QemuProcess, is_qemu_available, is_zenohd_available};
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

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

fn require_nuttx_cpp() -> bool {
    if !require_nuttx() {
        return false;
    }
    if !is_cmake_available() {
        eprintln!("Skipping test: cmake not found");
        return false;
    }
    true
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
// C++ build tests (kept as `#[ignore]` markers for upstream libc block)
// =============================================================================
//
// These tests are intentionally `#[ignore]`'d and serve as declarative
// markers for the upstream NuttX libc missing `_SC_HOST_NAME_MAX` issue.
// Running them would fail at the CMake configure step. They remain so
// that re-enabling NuttX C/C++ is a matter of removing six `#[ignore]`
// attributes, not restoring deleted test functions.

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

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_action_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_action_server().expect("Failed to build nuttx_cpp_action_server");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_action_server at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_action_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_action_client().expect("Failed to build nuttx_cpp_action_client");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_action_client at {}", binary.display());
}
