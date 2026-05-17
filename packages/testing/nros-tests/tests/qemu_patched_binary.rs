//! Phase 143.4 — patched `qemu-system-arm` smoke test.
//!
//! Asserts that `nros_tests::qemu::qemu_system_arm_path()` resolves to
//! the project-local `build/qemu/bin/qemu-system-arm` produced by
//! `just qemu setup-qemu`, that the patched binary reports a version
//! recent enough to satisfy the `-netdev dgram,local.type=unix,…`
//! gate (QEMU >= 7.2), and that `-netdev help` actually advertises
//! `dgram` as a backend type.
//!
//! Skips cleanly via `nros_tests::skip!` when the patched binary is
//! absent (e.g. a contributor that ran `just setup --tier=minimal`
//! and skipped the qemu module). The fallback path through system
//! `qemu-system-arm` keeps the rest of the test suite working and
//! is covered by the existing `qemu_supports_dgram_unix` gate inside
//! the dgram-using tests; it is not the focus of this test.

use std::path::PathBuf;

#[test]
fn test_qemu_system_arm_resolves_to_patched_build() {
    let path: PathBuf = nros_tests::qemu::qemu_system_arm_path().into();
    if !path.is_absolute() {
        nros_tests::skip!(
            "Patched qemu-system-arm not built (resolved to bare \
             `qemu-system-arm` on PATH). Run `just qemu setup-qemu`."
        );
    }
    if !path.exists() {
        nros_tests::skip!(
            "Resolved patched qemu-system-arm path does not exist: {} \
             — run `just qemu setup-qemu`.",
            path.display()
        );
    }
    assert!(
        path.ends_with("build/qemu/bin/qemu-system-arm")
            || std::env::var_os("QEMU_SYSTEM_ARM").is_some(),
        "qemu_system_arm_path() resolved to {} which is neither the \
         project-local `build/qemu/bin/qemu-system-arm` nor an explicit \
         `QEMU_SYSTEM_ARM` override",
        path.display()
    );
}

#[test]
fn test_patched_qemu_supports_dgram_unix() {
    let path: PathBuf = nros_tests::qemu::qemu_system_arm_path().into();
    if !path.is_absolute() || !path.exists() {
        nros_tests::skip!("Patched qemu-system-arm not built — run `just qemu setup-qemu`.");
    }
    let supports = nros_tests::fixtures::qemu_supports_dgram_unix();
    assert!(
        supports,
        "Patched qemu-system-arm at {} does not advertise `dgram` under \
         `-netdev help`. Verify the submodule pin is QEMU >= 7.2 and \
         that `just qemu setup-qemu` succeeded.",
        path.display()
    );
}

#[test]
fn test_patched_qemu_version_at_least_7_2() {
    let path: PathBuf = nros_tests::qemu::qemu_system_arm_path().into();
    if !path.is_absolute() || !path.exists() {
        nros_tests::skip!("Patched qemu-system-arm not built — run `just qemu setup-qemu`.");
    }
    let out = nros_tests::qemu::qemu_system_arm_cmd()
        .arg("--version")
        .output()
        .expect("Failed to invoke patched qemu-system-arm --version");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().unwrap_or("");
    // Format: "QEMU emulator version <major>.<minor>(.<patch>) (...)"
    let version = first
        .split_whitespace()
        .find(|tok| tok.chars().next().is_some_and(|c| c.is_ascii_digit()) && tok.contains('.'))
        .unwrap_or("");
    let mut parts = version.split('.');
    let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts
        .next()
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    assert!(
        (major, minor) >= (7, 2),
        "Patched qemu-system-arm at {} reports version {}.{} < 7.2 \
         (first --version line: {:?}). Bump the `third-party/qemu/qemu` \
         submodule pin.",
        path.display(),
        major,
        minor,
        first
    );
}
