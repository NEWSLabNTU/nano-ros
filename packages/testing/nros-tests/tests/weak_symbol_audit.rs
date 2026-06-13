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

use std::{fs, path::{Path, PathBuf}};

use nros_tests::project_root;

/// Path to the single source-of-truth allowlist, shared with the shell gate
/// `scripts/check-weak-symbols.sh` (run from `just check`). Each line:
/// `<expected weak-decl count> <repo-relative path>  # classification`
/// (override-default = a strong def is guaranteed elsewhere; optional-hook =
/// the weak no-op IS the intended fallback).
const ALLOWLIST_FILE: &str = "scripts/weak-symbols-allowlist.txt";

/// Parse `scripts/weak-symbols-allowlist.txt` → `path → expected-count`. Lines
/// are `<count> <repo-relative-path>  # classification`; `#` comments + blanks
/// are skipped.
fn load_allowlist(root: &Path) -> std::collections::HashMap<String, usize> {
    let raw = fs::read_to_string(root.join(ALLOWLIST_FILE))
        .unwrap_or_else(|e| panic!("read {ALLOWLIST_FILE}: {e}"));
    let mut map = std::collections::HashMap::new();
    for line in raw.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(count), Some(path)) = (it.next(), it.next()) else {
            continue;
        };
        let count: usize = count
            .parse()
            .unwrap_or_else(|_| panic!("{ALLOWLIST_FILE}: bad count in line: {line}"));
        map.insert(path.to_string(), count);
    }
    map
}

/// Recursively collect owned C/C++/asm sources under `packages/`, skipping
/// vendored / build / generated trees.
fn owned_sources(root: &Path) -> Vec<PathBuf> {
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
                Some("c")
                    | Some("cpp")
                    | Some("cc")
                    | Some("h")
                    | Some("hpp")
                    | Some("S")
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
    let allow = load_allowlist(&root);

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
            Some(expected) if *expected == count => {}
            Some(expected) => drifted.push(format!(
                "  {rel}: weak-decl count {count}, allowlist expects {expected} \
                 — a weak symbol was added/removed; re-audit + update {ALLOWLIST_FILE}."
            )),
            None => unexpected.push(format!(
                "  {rel}: {count} weak decl(s) — NEW unaudited weak-symbol site. \
                 Audit it (override-default vs optional-hook, where the strong def \
                 comes from), then add it to {ALLOWLIST_FILE}."
            )),
        }
    }

    // Stale allowlist entries (file moved / weak removed) — also forces review.
    let mut stale: Vec<String> = Vec::new();
    for p in allow.keys() {
        if !seen.contains(p) {
            stale.push(format!(
                "  {p}: allowlisted but no weak decl found (file moved/deleted, or \
                 weak removed) — drop it from {ALLOWLIST_FILE}."
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
