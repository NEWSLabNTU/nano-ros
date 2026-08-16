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

//! ## One scanner, not two (2026-08-16)
//!
//! This file used to re-implement the scan in Rust beside
//! `scripts/check-weak-symbols.sh`, with the allowlist as the only shared
//! artifact. Two spellings of one rule drift the moment either is fixed, and
//! that is exactly what happened: `35c603308` taught the SHELL scanner to strip
//! comments (the attribute is discussed in prose beside nearly every real use,
//! so phase-366's new sentences moved three counts with no new symbol) and
//! lowered the allowlist to the corrected numbers. The Rust copy still counted
//! comments, so the same tree passed `just check` and failed `test-all` —
//! a red that looks like a code regression and is a gate disagreement.
//!
//! So the test RUNS the shell gate rather than mirroring it. Coverage is
//! identical by construction (unaudited new site, drifted count, stale entry),
//! and there is one place left to fix when the rule changes.

use std::{path::PathBuf, process::Command};

use nros_tests::project_root;

/// The one scanner. `just check` runs it directly; this test runs the same
/// file, so the two lanes cannot disagree about what a weak declaration is.
const SCANNER: &str = "scripts/check-weak-symbols.sh";

#[test]
fn owned_weak_symbols_are_audited() {
    let root = project_root();
    let scanner: PathBuf = root.join(SCANNER);
    // An unmet precondition FAILS — a test that returns early on a missing
    // scanner reports PASS for a check that never ran.
    assert!(
        scanner.is_file(),
        "weak-symbol scanner missing: {}",
        scanner.display()
    );

    let out = Command::new("bash")
        .arg(&scanner)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", scanner.display()));

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "weak-symbol audit FAILED (issue 0050) — `bash {SCANNER}` exited {}:\n{stderr}{stdout}",
        out.status.code().unwrap_or(-1)
    );
    // Say what was covered: a scanner that silently matched zero files would
    // otherwise pass exactly like one that checked everything.
    print!("{stdout}");
    assert!(
        stdout.contains("audited weak-symbol files OK"),
        "scanner produced no coverage line — did it check anything?\n{stdout}{stderr}"
    );
}
