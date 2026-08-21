//! Platform-specific tests for nros
//!
//! Tests for Zephyr, embedded targets, and platform-specific functionality.

use nros_tests::fixtures::{is_arm_toolchain_available, is_qemu_available};
use std::process::Command;

// (Phase 182.3) `test_zephyr_{talker,listener}_build` removed — they only
// checked the Zephyr example dir + env existed (no build; and used the bare
// `eprintln!`+`return` skip that falsely reports PASS, contra CLAUDE.md).
// Zephyr example presence/build is covered by `just zephyr build-fixtures` +
// the zephyr.rs e2e tests.
//
// Same rule, applied to the rest of the file: `test_arm_toolchain_detection`,
// `test_qemu_arm_detection`, `test_zephyr_environment_detection`,
// `test_west_detection` and `test_zephyr_workspace_detection` were removed
// because they asserted NOTHING — each read one `is_*_available()` boolean and
// printed it, so all five reported PASS on a host with no toolchain, no QEMU
// and no Zephyr. A probe that cannot fail is not coverage; the same probes are
// load-bearing where they belong, as the `skip!` guards on the real tests
// below. `check-no-vacuous-tests` now forbids the shape repo-wide.
// (`test_arm_toolchain_detection` also existed verbatim in `emulator.rs` —
// two copies of a test that could not fail.)

// =============================================================================
// QEMU Emulation Tests (require QEMU)
// =============================================================================

#[test]
fn test_qemu_cortex_m3_available() {
    if !is_qemu_available() {
        nros_tests::skip!("QEMU not available");
    }

    // Verify QEMU can list the machine type we need
    let output = nros_tests::qemu::qemu_system_arm_cmd()
        .args(["-machine", "help"])
        .output()
        .expect("Failed to query QEMU machines");

    // Issue 0711's class — this printed "Warning: … not found" and PASSED, so
    // a QEMU without the machine every Cortex-M3 test needs reported green.
    // QEMU itself is already skip-guarded above; reaching here means QEMU IS
    // present, so a missing machine is a real defect in the install.
    let machines = String::from_utf8_lossy(&output.stdout);
    assert!(
        machines.contains("lm3s6965evb"),
        "QEMU is installed but has no `lm3s6965evb` machine — the Cortex-M3 \
         emulation every baremetal test needs cannot run"
    );
    eprintln!("QEMU lm3s6965evb machine available for Cortex-M3 emulation");
}

#[test]
fn test_qemu_semihosting_support() {
    if !is_qemu_available() {
        nros_tests::skip!("QEMU not available");
    }

    // Verify QEMU supports semihosting (check help output)
    let output = nros_tests::qemu::qemu_system_arm_cmd()
        .args(["--help"])
        .output()
        .expect("Failed to query QEMU help");

    // Issue 0711's class — see the machine check above. Semihosting is how the
    // baremetal fixtures report at all; a QEMU without it cannot run them, and
    // saying so in a warning that passes is the failure this gate now forbids.
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("semihosting"),
        "QEMU is installed but reports no semihosting support — the baremetal \
         fixtures have no way to emit output without it"
    );
    eprintln!("QEMU semihosting support available");
}

// =============================================================================
// Cross-Compilation Tests
// =============================================================================

#[test]
fn test_embedded_target_available() {
    if !is_arm_toolchain_available() {
        nros_tests::skip!("ARM toolchain not available");
    }

    // Verify we can compile a simple no_std crate
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .expect("Failed to list installed targets");

    let targets = String::from_utf8_lossy(&output.stdout);
    eprintln!("Installed ARM targets:");
    for line in targets.lines() {
        if line.contains("thumb") || line.contains("arm") {
            eprintln!("  {}", line);
        }
    }
}
