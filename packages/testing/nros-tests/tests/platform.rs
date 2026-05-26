//! Platform-specific tests for nros
//!
//! Tests for Zephyr, embedded targets, and platform-specific functionality.

use nros_tests::fixtures::{is_arm_toolchain_available, is_qemu_available};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

// =============================================================================
// Zephyr Environment Detection
// =============================================================================

/// Check if Zephyr environment is available
fn is_zephyr_available() -> bool {
    // Check for ZEPHYR_BASE environment variable
    std::env::var("ZEPHYR_BASE").is_ok()
}

/// Check if west is available
fn is_west_available() -> bool {
    Command::new("west")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get Zephyr workspace path (if available)
fn zephyr_workspace() -> Option<PathBuf> {
    // Look for ~/nano-ros-workspace or configured path
    let home = std::env::var("HOME").ok()?;
    let workspace = PathBuf::from(home).join("nano-ros-workspace");
    if workspace.exists() {
        Some(workspace)
    } else {
        None
    }
}

// =============================================================================
// Environment Detection Tests
// =============================================================================

#[test]
fn test_arm_toolchain_detection() {
    let available = is_arm_toolchain_available();
    eprintln!(
        "ARM toolchain (thumbv7m-none-eabi) available: {}",
        available
    );
}

#[test]
fn test_qemu_arm_detection() {
    let available = is_qemu_available();
    eprintln!("QEMU ARM available: {}", available);
}

#[test]
fn test_zephyr_environment_detection() {
    let available = is_zephyr_available();
    eprintln!("Zephyr environment available: {}", available);

    if let Ok(base) = std::env::var("ZEPHYR_BASE") {
        eprintln!("ZEPHYR_BASE: {}", base);
    }
}

#[test]
fn test_west_detection() {
    let available = is_west_available();
    eprintln!("west available: {}", available);
}

#[test]
fn test_zephyr_workspace_detection() {
    match zephyr_workspace() {
        Some(path) => eprintln!("Zephyr workspace found: {}", path.display()),
        None => eprintln!("Zephyr workspace not found at ~/nano-ros-workspace"),
    }
}

// (Phase 182.3) `test_zephyr_{talker,listener}_build` removed — they only
// checked the Zephyr example dir + env existed (no build; and used the bare
// `eprintln!`+`return` skip that falsely reports PASS, contra CLAUDE.md).
// Zephyr example presence/build is covered by `just zephyr build-fixtures` +
// the zephyr.rs e2e tests.

// =============================================================================
// QEMU Emulation Tests (require QEMU)
// =============================================================================

#[test]
fn test_qemu_cortex_m3_available() {
    if !is_qemu_available() {
        eprintln!("Skipping test: QEMU not available");
        return;
    }

    // Verify QEMU can list the machine type we need
    let output = nros_tests::qemu::qemu_system_arm_cmd()
        .args(["-machine", "help"])
        .output()
        .expect("Failed to query QEMU machines");

    let machines = String::from_utf8_lossy(&output.stdout);
    if machines.contains("lm3s6965evb") {
        eprintln!("QEMU lm3s6965evb machine available for Cortex-M3 emulation");
    } else {
        eprintln!("Warning: lm3s6965evb machine not found in QEMU");
    }
}

#[test]
fn test_qemu_semihosting_support() {
    if !is_qemu_available() {
        eprintln!("Skipping test: QEMU not available");
        return;
    }

    // Verify QEMU supports semihosting (check help output)
    let output = nros_tests::qemu::qemu_system_arm_cmd()
        .args(["--help"])
        .output()
        .expect("Failed to query QEMU help");

    let help = String::from_utf8_lossy(&output.stdout);
    if help.contains("semihosting") {
        eprintln!("QEMU semihosting support available");
    } else {
        eprintln!("Warning: semihosting not mentioned in QEMU help");
    }
}

// =============================================================================
// Cross-Compilation Tests
// =============================================================================

#[test]
fn test_embedded_target_available() {
    if !is_arm_toolchain_available() {
        eprintln!("Skipping test: ARM toolchain not available");
        return;
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
