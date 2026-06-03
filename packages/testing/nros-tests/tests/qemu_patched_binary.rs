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
//!
//! Phase 214.O.2 — skip-gate is hoisted into `require_patched_qemu()`
//! so every test body starts with one statement that either returns
//! the resolved path or panics with `[SKIPPED] …`. The previous
//! skip-then-assert shape was correct (`nros_tests::skip!` is a
//! `panic!()`-shaped macro so the subsequent asserts never run on a
//! missing SDK), but a reader could not tell at a glance whether the
//! assert was reachable on the skip path. The hoisted gate makes the
//! intent explicit, matches the Phase 212.H test pattern, and removes
//! the in-body `if !path.is_absolute() { skip!(…) } if !path.exists()
//! { skip!(…) }` duplication.

use std::path::PathBuf;

/// Resolve `qemu_system_arm_path()` to an absolute, existing patched
/// binary, or `nros_tests::skip!` with the canonical "run `just qemu
/// setup-qemu`" hint. Callers can then proceed unconditionally — every
/// test body below assumes the returned path is a real file.
fn require_patched_qemu() -> PathBuf {
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
    path
}

#[test]
fn test_qemu_system_arm_resolves_to_patched_build() {
    let path = require_patched_qemu();
    // Accept the project-local build, an explicit override, or the
    // `nros setup` store qemu (the same patched `11.0.0-nros*` dist).
    let from_store =
        nros_tests::nros_store_bin("qemu", "qemu-system-arm").is_some_and(|s| s == path);
    assert!(
        path.ends_with("build/qemu/bin/qemu-system-arm")
            || std::env::var_os("QEMU_SYSTEM_ARM").is_some()
            || from_store,
        "qemu_system_arm_path() resolved to {} which is neither the \
         project-local `build/qemu/bin/qemu-system-arm`, an explicit \
         `QEMU_SYSTEM_ARM` override, nor the `nros setup` store qemu",
        path.display()
    );
}

#[test]
fn test_patched_qemu_supports_dgram_unix() {
    let path = require_patched_qemu();
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
    let path = require_patched_qemu();
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
