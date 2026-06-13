//! Issue 0050 — weak-symbol audit gate.
//!
//! Weak symbols (`__attribute__((weak))` in C/C++, `.weak` in asm) are
//! bug-prone: which definition the linker keeps depends on archive order,
//! `--gc-sections` and `--whole-archive`, and a weak symbol can be silently
//! dropped or the wrong copy chosen with **no link error** — a runtime
//! mis-behaviour (cf. the #48-class "registered into the wrong instance"
//! hazard, and the 155.A const-weak-inlining bug noted in `threadx_hooks.c`).
//!
//! This is the **source-level guard**: every owned C/C++/asm file that defines
//! weak symbols is on an audited allowlist with its expected weak-decl count +
//! classification. The gate fails when:
//!   - an owned source file outside the allowlist introduces a weak symbol
//!     (a new, unaudited weak site slipped in), or
//!   - an allowlisted file's weak-decl count drifts (a weak symbol was
//!     added/removed without updating the audit) — forces re-review.
//!
//! Vendored trees (zenoh-pico, mbedtls, third-party) are excluded — their weak
//! usage is upstream's concern, not this codebase's.
//!
//! Scope NOT covered here (issue 0050 follow-ups): the per-platform *final
//! image* checker (assert each override-default weak symbol is actually
//! overridden by a strong def in the linked artifact, robust to
//! `--gc-sections`/`--whole-archive`) and the reduction of fragile weak
//! defaults to define-once / explicit-registration (RFC-0042 D3). The
//! allowlist below is the audit those phases build on.

use std::{fs, path::PathBuf};

use nros_tests::project_root;

/// `(relative path, expected weak-decl line count, classification)`.
///
/// Classification: **override-default** = a strong def is guaranteed elsewhere
/// (board / app / cmake-generated stub); **optional-hook** = the weak no-op IS
/// the intended fallback. The "decl count" counts lines bearing
/// `__attribute__((weak))` or a `.weak ` directive (a couple are doc-comment
/// mentions in the same file — counted for stability, so any edit re-triggers
/// review).
const ALLOWLIST: &[(&str, usize, &str)] = &[
    (
        "packages/core/nros-c/c-stubs/weak_register_backends.c",
        3,
        "override-default: cmake-generated strong `nros_app_register_backends` \
         (NanoRosLink.cmake) overrides the weak default; no-op fallbacks \
         `nros_platform_log_{write,flush}` satisfy a no-platform link path.",
    ),
    (
        "packages/core/nros-cpp/c-stubs/weak_register_backends.c",
        1,
        "override-default: cmake-generated strong `nros_app_register_backends` \
         for nros-cpp-only links.",
    ),
    (
        "packages/core/nros-cpp/include/nros/main.hpp",
        1,
        "optional-hook: `nros_board_network_wait` weak no-op (header inline def); \
         a board strong-overrides it with a real link-up wait, else no-wait is \
         the intended default.",
    ),
    (
        "packages/boards/nros-board-common/c/threadx_hooks.c",
        7,
        "override-default: overlay strong-overrides `nros_board_app_stack_size`/\
         `_priority`/`nros_board_init_eth`/`app_main`; optional-hook: \
         `nros_board_log`/`nros_board_compute_rng_seed` no-ops. (155.A const-weak \
         inlining bug fixed — `const` qualifier dropped.)",
    ),
    (
        "packages/boards/nros-board-freertos/c/network_glue.c",
        2,
        "override-default: a board (LAN9118 / STM ETH) supplies strong \
         `nros_board_register_netif`/`nros_board_poll_netif`; the weak default \
         is the no-Ethernet fallback (returns -1 / no-op).",
    ),
    (
        "packages/boards/nros-board-threadx-qemu-riscv64/c/tx_initialize_low_level.S",
        1,
        "override-default: the board C provides a strong `_tx_initialize_low_level`; \
         the asm `.weak` is the ThreadX port fallback.",
    ),
    (
        "packages/core/nros-platform-threadx/src/platform.c",
        8,
        "optional-hook: weak libc stubs (`open`/`close`/`read`/`write`/`lseek`/\
         `pipe`/`stdin`) that the backend strong-overrides when it supplies real \
         POSIX; the weak no-ops keep the link resolvable until then.",
    ),
    (
        "packages/dds/nros-rmw-cyclonedds/src/vtable.cpp",
        1,
        "optional-hook: `nros_rmw_cyclonedds_register_app_descriptors` weak no-op; \
         the app's generated descriptor TU strong-overrides it.",
    ),
    (
        "packages/px4/nros-rmw-uorb/src/callback_default.cpp",
        2,
        "override-default: `px4_callback_glue.cpp` strong-defines \
         `nros_orb_{register,unregister}_callback`; the weak default returns -1 \
         (subscriber-push unsupported) for links without the glue.",
    ),
    (
        "packages/zpico/zpico-sys/c/zpico/platform_aliases.c",
        9,
        "override-default: a board strong-overrides `_z_*_serial_*` + \
         `smoltcp_{init,cleanup}`; the weak stubs satisfy the zenoh-pico link \
         when serial / smoltcp transports are absent.",
    ),
];

/// Recursively collect owned C/C++/asm sources under `packages/`, skipping
/// vendored / build / generated trees.
fn owned_sources(root: &PathBuf) -> Vec<PathBuf> {
    fn skip_dir(name: &str) -> bool {
        matches!(
            name,
            "target" | "build" | "generated" | "zenoh-pico" | "mbedtls" | "third-party" | ".git"
        )
    }
    let mut out = Vec::new();
    let mut stack = vec![root.join("packages")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !skip_dir(&name) {
                    stack.push(path);
                }
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("c") | Some("cpp") | Some("cc") | Some("h") | Some("hpp") | Some("S")
                    | Some("s")
            ) {
                out.push(path);
            }
        }
    }
    out
}

/// Count lines bearing a weak declaration / directive.
fn weak_decl_count(text: &str) -> usize {
    text.lines()
        .filter(|l| l.contains("__attribute__((weak))") || l.contains(".weak "))
        .count()
}

#[test]
fn owned_weak_symbols_are_audited() {
    let root = project_root();
    let allow: std::collections::HashMap<&str, (usize, &str)> = ALLOWLIST
        .iter()
        .map(|(p, n, c)| (*p, (*n, *c)))
        .collect();

    let mut unexpected: Vec<String> = Vec::new();
    let mut drifted: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for path in owned_sources(&root) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let count = weak_decl_count(&text);
        if count == 0 {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        seen.insert(rel.clone());
        match allow.get(rel.as_str()) {
            Some((expected, _)) if *expected == count => {}
            Some((expected, _)) => drifted.push(format!(
                "  {rel}: weak-decl count {count}, allowlist expects {expected} \
                 — a weak symbol was added/removed; re-audit + update ALLOWLIST."
            )),
            None => unexpected.push(format!(
                "  {rel}: {count} weak decl(s) — NEW unaudited weak-symbol site. \
                 Audit it (override-default vs optional-hook, where the strong def \
                 comes from), then add it to ALLOWLIST in this test."
            )),
        }
    }

    // Stale allowlist entries (file moved / weak removed) — also forces review.
    let mut stale: Vec<String> = Vec::new();
    for (p, _, _) in ALLOWLIST {
        if !seen.contains(*p) {
            stale.push(format!(
                "  {p}: allowlisted but no weak decl found (file moved/deleted, or \
                 weak removed) — drop it from ALLOWLIST."
            ));
        }
    }

    let mut msg = String::new();
    if !unexpected.is_empty() {
        msg.push_str("UNEXPECTED weak-symbol sites (issue 0050):\n");
        msg.push_str(&unexpected.join("\n"));
        msg.push('\n');
    }
    if !drifted.is_empty() {
        msg.push_str("DRIFTED weak-decl counts:\n");
        msg.push_str(&drifted.join("\n"));
        msg.push('\n');
    }
    if !stale.is_empty() {
        msg.push_str("STALE allowlist entries:\n");
        msg.push_str(&stale.join("\n"));
        msg.push('\n');
    }
    assert!(msg.is_empty(), "{msg}");
}
