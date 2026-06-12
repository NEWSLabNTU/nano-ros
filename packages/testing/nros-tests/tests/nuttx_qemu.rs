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
//! - `NUTTX_DIR` env var pointing to a NuttX source checkout
//! - Nightly Rust toolchain with `armv7a-nuttx-eabihf` target
//! - `qemu-system-arm` with virt machine support
//! - zenohd: `just build-zenohd`
//!
//! Run with: `just test-nuttx`
//! Or: `cargo nextest run -p nros-tests --test nuttx_qemu`

use nros_tests::fixtures::{
    QemuProcess, is_qemu_available, is_zenohd_available,
    nuttx::{
        build_nuttx_c_action_client, build_nuttx_c_action_server, build_nuttx_c_listener,
        build_nuttx_c_service_client, build_nuttx_c_service_server, build_nuttx_c_talker,
        build_nuttx_cpp_action_client, build_nuttx_cpp_action_server, build_nuttx_cpp_listener,
        build_nuttx_cpp_service_client, build_nuttx_cpp_service_server, build_nuttx_cpp_talker,
        is_arm_gcc_available, is_cmake_available, is_nuttx_available, is_nuttx_configured,
        is_nuttx_toolchain_available, nuttx_kernel_path,
    },
};
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Skip test if NuttX build prerequisites are not available
fn require_nuttx() -> bool {
    if !is_nuttx_available() {
        eprintln!("Skipping test: NUTTX_DIR not set or invalid");
        eprintln!("Run: just nuttx setup, then load .envrc or set NUTTX_DIR");
        return false;
    }
    if !is_nuttx_configured() {
        eprintln!(
            "Skipping test: NuttX not configured ($NUTTX_DIR/include/nuttx/config.h missing)"
        );
        eprintln!("Run: scripts/nuttx/build-nuttx.sh");
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
// (Phase 182.3) `test_nuttx_all_examples_build` removed — it rebuilt every
// NuttX **Rust** example, which `build-all` / `build-test-fixtures` already do
// before `test-all` (the `_require-fixtures` preflight). The per-role binaries
// are consumed by the `rtos_e2e` Platform__Nuttx tests. (The NuttX C++
// build/boot tests below keep their own `build_nuttx_cpp_*` coverage.)
// =============================================================================

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
        nros_tests::skip!("NUTTX_DIR not set");
    }
    let kernel = match nuttx_kernel_path() {
        Some(k) => k,
        None => {
            nros_tests::skip!("NuttX kernel not built ($NUTTX_DIR/nuttx)");
        }
    };
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
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
        eprintln!("Build: scripts/nuttx/build-nuttx.sh");
    }
}

// =============================================================================
// C++ build tests (Phase 238.A — bootable-ELF carrier wired)
// =============================================================================
//
// Phase 238.A — the bootable-ELF carrier is now wired (the
// `nano_ros_node_register` NuttX branch synthesises a single-node entry TU and
// delegates to `nros_platform_link_app` → cargo `nros-nuttx-ffi`), so every C++
// example produces a real `build-zenoh/nuttx_cpp_<name>` kernel ELF. These
// build tests resolve it (build coverage). The talker/listener pub/sub pair is
// additionally proven to boot + route over zenoh in QEMU; service/action build
// + boot + *register* but do not execute (interpreter limitation), and the
// rtos_e2e `Nuttx × Cpp` E2E cases need listener-side observability — both
// deferred (see docs/roadmap/phase-238-nuttx-cpp-e2e-enablement.md).

#[test]
fn test_nuttx_cpp_talker_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_talker().expect("Failed to build nuttx_cpp_talker");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_talker at {}", binary.display());
}

#[test]
fn test_nuttx_cpp_listener_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_listener().expect("Failed to build nuttx_cpp_listener");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_listener at {}", binary.display());
}

#[test]
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
fn test_nuttx_cpp_action_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_action_server().expect("Failed to build nuttx_cpp_action_server");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_action_server at {}", binary.display());
}

#[test]
fn test_nuttx_cpp_action_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_action_client().expect("Failed to build nuttx_cpp_action_client");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_action_client at {}", binary.display());
}

// =============================================================================
// C build tests (Phase 238.C — bootable-ELF carrier wired for LANGUAGE C)
// =============================================================================
//
// Phase 238.C — the carrier now fires for `LANGUAGE C` (DEPLOY nuttx) too: the
// generated entry stays C++ (it drives the header-only C++ EntryNodeRuntime),
// the declarative C node (`Talker.c`) is compiled as C by the mixed-language
// cargo build and linked via its C-linkage register symbol. Every C example
// produces a real `build-zenoh/nuttx_c_<name>` kernel ELF. The talker/listener
// pub/sub pair is proven to boot + exchange `/chatter` Int32 over zenoh
// (Published/Received with matching values); service/action build + boot +
// register but do not execute (interpreter limit — see phase-238).

#[test]
fn test_nuttx_c_talker_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_c_talker().expect("Failed to build nuttx_c_talker");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_c_talker at {}", binary.display());
}

#[test]
fn test_nuttx_c_listener_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_c_listener().expect("Failed to build nuttx_c_listener");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_c_listener at {}", binary.display());
}

#[test]
fn test_nuttx_c_service_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_c_service_server().expect("Failed to build nuttx_c_service_server");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_c_service_server at {}", binary.display());
}

#[test]
fn test_nuttx_c_service_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_c_service_client().expect("Failed to build nuttx_c_service_client");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_c_service_client at {}", binary.display());
}

#[test]
fn test_nuttx_c_action_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_c_action_server().expect("Failed to build nuttx_c_action_server");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_c_action_server at {}", binary.display());
}

#[test]
fn test_nuttx_c_action_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_c_action_client().expect("Failed to build nuttx_c_action_client");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_c_action_client at {}", binary.display());
}
