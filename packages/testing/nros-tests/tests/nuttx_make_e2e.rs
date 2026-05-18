//! Phase 157.C — NuttX external-app make-build smoke + E2E parity.
//!
//! Validates the make-based NuttX build path (the canonical
//! `apps/external/*/Make.defs` + `apps/external/*/Kconfig`
//! discovery) by staging the nano-ros integration shell + every
//! C/C++ example as `apps/external/nano-ros-*-<lang>` siblings,
//! running `make` from the configured NuttX tree, and asserting
//! the resulting `nuttx` binary contains the built-in command
//! symbols for each example.
//!
//! Scope: smoke-test the wrapper trio (`Kconfig + Make.defs +
//! Makefile`) per example landed in Phase 157.A. End-to-end runtime
//! delivery parity with the cmake-built `rtos_e2e Platform_Nuttx`
//! tests is the follow-up — gated on a QEMU launch of the
//! make-built binary which needs additional fixture infrastructure.
//!
//! Skips cleanly via `nros_tests::skip!` when:
//!   * `NUTTX_DIR` is unset / kernel not configured.
//!   * `NUTTX_APPS_DIR` is unset / apps tree missing.
//!   * `arm-none-eabi-gcc` not on PATH.
//!   * `just nuttx build-fixtures-make` hasn't been run yet (the
//!     `nuttx` ELF doesn't exist at `$NUTTX_DIR/nuttx`).

use std::{path::PathBuf, process::Command};

use nros_tests::fixtures::nuttx::{is_arm_gcc_available, is_nuttx_available, is_nuttx_configured};

fn nuttx_dir() -> Option<PathBuf> {
    std::env::var("NUTTX_DIR").ok().map(PathBuf::from)
}

// Phase 157.C.15 — C examples only. CPP example linking blocked
// on the per-package Rust FFI staticlib build (157.C.16) — the
// codegen output crate at `generated/<pkg>/Cargo.toml` needs
// cargo build + EXTRA_LIBS append before nuttx_cpp_*_main can
// resolve. Extend this list once .C.16 lands.
const EXPECTED_PROGNAMES: &[&str] = &[
    "nuttx_c_talker",
    "nuttx_c_listener",
    "nuttx_c_service_server",
    "nuttx_c_service_client",
    "nuttx_c_action_server",
    "nuttx_c_action_client",
];

#[test]
fn nuttx_external_apps_link_into_kernel_binary() {
    if !is_arm_gcc_available() {
        nros_tests::skip!("arm-none-eabi-gcc not on PATH");
    }
    if !is_nuttx_available() || !is_nuttx_configured() {
        nros_tests::skip!(
            "NUTTX_DIR unset or kernel not configured — run \
             `just nuttx setup` first"
        );
    }
    let Some(nuttx_dir) = nuttx_dir() else {
        nros_tests::skip!("NUTTX_DIR unset");
    };
    let kernel = nuttx_dir.join("nuttx");
    if !kernel.exists() {
        nros_tests::skip!(
            "NuttX kernel binary not built at {} — run `just nuttx \
             build-fixtures-make` first to stage + build all 12 nano-ros \
             external-app examples into a single nuttx ELF",
            kernel.display()
        );
    }
    // Probe the staged apps/external/nano-ros symlink as proxy for
    // "build-fixtures-make has been run against this NuttX tree".
    // Without it, $NUTTX_DIR/nuttx may exist from an unrelated
    // build (e.g. `just nuttx setup` builds a vanilla kernel) and
    // the test would false-fail on missing symbols.
    let apps_dir = std::env::var("NUTTX_APPS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| nuttx_dir.parent().unwrap().join("nuttx-apps"));
    let staged = apps_dir.join("external/nano-ros");
    if !staged.exists() {
        nros_tests::skip!(
            "nano-ros not staged under {} — run `just nuttx \
             build-fixtures-make` first (the existing {} binary \
             was built without the nano-ros external apps)",
            staged.display(),
            kernel.display()
        );
    }

    // `nm` dump — built-in commands are static symbols named
    // `<PROGNAME>_main` after Application.mk's `-Dmain=<PROGNAME>_main`
    // rename trick. Symbols may be local or weak; `nm -A` covers
    // every section + symbol class.
    let output = Command::new("nm")
        .arg("-A")
        .arg(&kernel)
        .output()
        .expect("nm should be on PATH");
    if !output.status.success() {
        panic!(
            "nm {} failed: stderr={}",
            kernel.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let symbols = String::from_utf8_lossy(&output.stdout);

    let mut missing = Vec::new();
    for progname in EXPECTED_PROGNAMES {
        let symbol = format!("{progname}_main");
        if !symbols.contains(&symbol) {
            missing.push(symbol);
        }
    }

    assert!(
        missing.is_empty(),
        "Expected NuttX kernel at {} to link every nano-ros example as a \
         built-in command (Application.mk `-Dmain=<PROGNAME>_main` rename). \
         Missing symbols ({} of {}):\n  {}\n\n\
         Hint: run `just nuttx build-fixtures-make` to (re-)stage the \
         apps/external/ tree + rebuild.",
        kernel.display(),
        missing.len(),
        EXPECTED_PROGNAMES.len(),
        missing.join("\n  ")
    );
    eprintln!(
        "[PASS] all {} nano-ros example PROGNAMEs linked into {}",
        EXPECTED_PROGNAMES.len(),
        kernel.display()
    );
}
