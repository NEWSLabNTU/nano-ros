//! Phase 275 W6 (#102) — silent-gap gate for the example fixture matrix.
//!
//! "No silent caps": every example project claimed in the tree must be built
//! by *some* fixture mechanism, **or** be a tracked exception with a reason.
//! A new example that appears with neither a fixture nor an allowlist entry
//! fails this test — closing the loophole where a matrix-listed example is
//! never built by CI yet still reads as "covered".
//!
//! ## What counts as an example project (a "matrix cell")
//!
//! A directory under `examples/` that carries a `package.xml` and is a
//! standalone copy-out project (per CLAUDE.md "Examples = Standalone
//! Projects"). Excluded from per-leaf gating:
//! - `examples/templates/`, `examples/bridges/` — sibling categories with
//!   their own conventions (same carve-out as `examples_canonical_shape.rs`).
//! - `examples/workspaces/<ws>/src/<pkg>` — workspace *member* packages are
//!   not independent cells; the workspace is built as a unit by
//!   `scripts/build/workspace-fixtures-build.sh` and exercised end-to-end by
//!   the phase-263 `ws-*_e2e.rs` tests. Gating each member would double-count.
//!
//! ## Coverage sources (a leaf is "covered" if any fires)
//!
//! 1. `examples/fixtures.toml` — every `dir = "examples/…"` row (read live).
//! 2. Zephyr role driver — `scripts/build/zephyr-fixture-leaves.sh` +
//!    `fixture-matrix.sh` build `zephyr/{c,cpp,rust}/{6 roles}`.
//! 3. `scripts/build/compile-check-fixtures.sh` — every `examples/…` token
//!    (the `CARGO_CHECK_EXAMPLES` cross-check dirs etc.), read live.
//! 4. Test-driven builders — dirs a test harness builds directly rather than
//!    via a manifest row (e.g. freertos `talker_entry` via
//!    `freertos_run_plan_runtime.rs`). Enumerated in `TEST_DRIVEN_BUILDERS`.
//!
//! ## Tracked exceptions (`ALLOWLIST`)
//!
//! Examples deliberately not (yet) covered, each with a reason and the work
//! item that will close it. As Phase 275/276 land fixtures, entries move from
//! `ALLOWLIST` into a coverage source above — the test fails if an allowlisted
//! dir later gains a fixture (dead exception) *or* an uncovered dir lacks an
//! entry (new silent gap). Both directions keep the list honest.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Top-level `examples/` children skipped wholesale.
const SKIP_TOP_LEVEL: &[&str] = &["templates", "bridges"];

// phase-350 W1.c — the `ZEPHYR_LANGS` x `ZEPHYR_ROLES` constants that lived
// here are GONE. They were the THIRD spelling of the zephyr matrix: the bash
// loops in `fixture-matrix.sh` were the second, `examples/fixtures.toml` is the
// first and only one now. This gate reads the rows (`lane::west_leaves()`), so
// a leaf added to or removed from the manifest changes what it considers
// covered without anyone remembering to edit a constant here.
//
// That is the whole failure mode this file exists to prevent, applied to
// itself: a coverage gate whose "covered" set is hand-maintained reports green
// for a dir nothing builds the moment the two drift.

/// Dirs built directly by a test harness (no `fixtures.toml` row). Keep the
/// harness path in the comment so the pairing is auditable.
const TEST_DRIVEN_BUILDERS: &[&str] = &[
    // phase-338 W2 — the 6 freertos `*-entry` dirs are GONE: each collapsed
    // into its role package, which already carries a `fixtures.toml` row. They
    // are therefore covered by a source above and must NOT be listed here (a
    // dir covered twice is a stale exception, per this list's own rule).
    // `just freertos build-examples` still builds the default-`target/` binary
    // that `freertos_run_plan_runtime.rs` locates — a second build of a
    // covered dir, not a second dir.
    // phase-350 W3 (issue 0537) — `zephyr/{rust,cpp}/talker-aemv8r` used to sit
    // here, "built by `just zephyr build-fvp-aemv8r-cyclonedds{,-rust}`". Both
    // examples and both recipes are GONE.
    //
    // The entry was honest about the problem and could not fix it: phase-321
    // W3.c had already corrected the comment to say the named runners
    // (`fvp_runtime.rs` / `fvp_runtime_rust.rs`) were DELETED by phase-298 W4,
    // so for months this list recorded two examples nothing ran, and the
    // justfile still read as a complete lane. An allowlist can only excuse a
    // gap; retiring the code is what closes it.
];

/// Tracked exceptions: (dir relative to `examples/`, reason). A dir here must
/// NOT also be covered by a source above (that would be a stale exception).
///
/// 275 W1 `*_entry` coverage is complete: freertos via the run-plan harness
/// (TEST_DRIVEN_BUILDERS), threadx-linux via `[[fixture]]` rows +
/// tests/threadx_linux_entry_build.rs, and nuttx via `[[fixture]]` rows +
/// tests/nuttx_entry_build.rs (issue #127 resolved by the board-centric
/// image link, RFC-0032 "third leg").
///
/// Empty: the px4 xrce examples that once sat here now compile-check via
/// `scripts/build/compile-check-fixtures.sh` (its px4 leg generates `px4_msgs`
/// from the vendored PX4-Autopilot `.msg` tree, gated on the submodule).
const ALLOWLIST: &[(&str, &str)] = &[];

/// Recursive walk collecting dirs that contain `package.xml`.
fn collect_pkg_dirs(root: &Path) -> Vec<PathBuf> {
    fn skip_dir(name: &str) -> bool {
        // Skip build/target OUTPUT trees (`build-fixtures/`, `target-cyclonedds/`,
        // `target-fixtures/`, …) — descending them is the difference between a
        // sub-second walk and a 15-minute one once fixtures are built, and they
        // can carry vendored `package.xml` that would read as false leaves.
        //
        // This comment stated the cost precisely and stayed in this file, while
        // `examples_canonical_shape.rs` went on to pay it. Hence the shared
        // predicate: the rule is one thing, so it gets one implementation.
        nros_tests::treewalk::is_skipped_dir(name)
    }
    fn walk(dir: &Path, acc: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut has_pkg_xml = false;
        let mut subdirs = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if p.is_dir() {
                if !skip_dir(name) {
                    subdirs.push(p);
                }
            } else if name == "package.xml" {
                has_pkg_xml = true;
            }
        }
        if has_pkg_xml {
            acc.push(dir.to_path_buf());
        }
        for s in subdirs {
            walk(&s, acc);
        }
    }
    let mut acc = Vec::new();
    walk(root, &mut acc);
    acc
}

/// Extract every `examples/…`-relative dir referenced in a build script or
/// manifest text (matches `examples/<path>` tokens).
fn examples_rel_tokens(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = text.as_bytes();
    let needle = b"examples/";
    let mut i = 0;
    while let Some(pos) = text[i..].find("examples/") {
        let start = i + pos;
        let mut end = start + needle.len();
        while end < bytes.len() {
            let c = bytes[end];
            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'/' | b'-') {
                end += 1;
            } else {
                break;
            }
        }
        let tok = &text[start..end];
        // strip a trailing filename component if the token points at a file
        if let Some(rel) = tok.strip_prefix("examples/") {
            out.insert(rel.trim_end_matches('/').to_string());
        }
        i = end;
    }
    out
}

#[test]
fn every_example_has_a_fixture_or_tracked_exception() {
    let root = nros_tests::project_root();
    let examples_root = root.join("examples");
    if !examples_root.is_dir() {
        nros_tests::skip!("examples/ missing at {}", examples_root.display());
    }

    // ---- Build the covered set. ------------------------------------------
    let mut covered: BTreeSet<String> = BTreeSet::new();

    // 1. fixtures.toml dir rows.
    let fixtures_toml = fs::read_to_string(examples_root.join("fixtures.toml"))
        .expect("read examples/fixtures.toml");
    for line in fixtures_toml.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("dir") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let v = rest.trim().trim_matches('"');
                if let Some(rel) = v.strip_prefix("examples/") {
                    covered.insert(rel.trim_end_matches('/').to_string());
                }
            }
        }
    }

    // 2. Zephyr west leaves — read from the manifest, not restated here.
    //
    // `west_leaves()` yields each row's repo-relative `dir`; this gate keys on
    // paths relative to `examples/`, so only the `examples/**` leaves map (the
    // logging-smoke leaf lives under `packages/testing/`, and is not an example
    // dir this walk ever sees).
    for leaf in nros_tests::fixtures::lane::west_leaves() {
        if let Some(rel) = leaf.dir.strip_prefix("examples/") {
            covered.insert(rel.trim_end_matches('/').to_string());
        }
    }

    // 3. compile-check-fixtures.sh referenced example dirs.
    let compile_check = fs::read_to_string(root.join("scripts/build/compile-check-fixtures.sh"))
        .unwrap_or_default();
    covered.extend(examples_rel_tokens(&compile_check));

    // 4. Test-driven builders.
    for d in TEST_DRIVEN_BUILDERS {
        covered.insert((*d).to_string());
    }

    let allow: BTreeSet<&str> = ALLOWLIST.iter().map(|(d, _)| *d).collect();

    // ---- Enumerate leaves + classify. ------------------------------------
    let mut leaves: Vec<String> = Vec::new();
    for e in fs::read_dir(&examples_root)
        .expect("read examples/")
        .flatten()
    {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if SKIP_TOP_LEVEL.contains(&name) {
            continue;
        }
        for pkg in collect_pkg_dirs(&p) {
            let rel = pkg
                .strip_prefix(&examples_root)
                .unwrap_or(&pkg)
                .to_string_lossy()
                .replace('\\', "/");
            // Workspace member pkgs are covered by the workspace unit, not
            // gated individually.
            if rel.starts_with("workspaces/") {
                continue;
            }
            leaves.push(rel);
        }
    }
    leaves.sort();
    leaves.dedup();

    // Uncovered-and-not-allowlisted → silent gap (fail).
    let mut silent_gaps = Vec::new();
    // Allowlisted-but-now-covered → stale exception (fail; tidy the list).
    let mut stale_exceptions = Vec::new();

    for leaf in &leaves {
        let is_covered = covered.contains(leaf);
        let is_allowed = allow.contains(leaf.as_str());
        if !is_covered && !is_allowed {
            silent_gaps.push(leaf.clone());
        }
        if is_covered && is_allowed {
            stale_exceptions.push(leaf.clone());
        }
    }

    // Allowlist entries for dirs that no longer exist → also stale.
    let leaf_set: BTreeSet<&str> = leaves.iter().map(|s| s.as_str()).collect();
    let mut dangling_allow = Vec::new();
    for (d, _) in ALLOWLIST {
        if !leaf_set.contains(d) {
            dangling_allow.push((*d).to_string());
        }
    }

    let mut msg = String::new();
    if !silent_gaps.is_empty() {
        msg.push_str(&format!(
            "\n{} example(s) built by NO fixture mechanism and NOT in the \
             tracked-exception ALLOWLIST (silent coverage gap — add a fixture \
             row or an ALLOWLIST entry with a reason):\n",
            silent_gaps.len()
        ));
        for g in &silent_gaps {
            msg.push_str(&format!("  - {g}\n"));
        }
    }
    if !stale_exceptions.is_empty() {
        msg.push_str(&format!(
            "\n{} ALLOWLIST entr(ies) now have a real fixture (stale exception \
             — remove from ALLOWLIST):\n",
            stale_exceptions.len()
        ));
        for s in &stale_exceptions {
            msg.push_str(&format!("  - {s}\n"));
        }
    }
    if !dangling_allow.is_empty() {
        msg.push_str(&format!(
            "\n{} ALLOWLIST entr(ies) point at a dir that no longer exists \
             (remove from ALLOWLIST):\n",
            dangling_allow.len()
        ));
        for d in &dangling_allow {
            msg.push_str(&format!("  - {d}\n"));
        }
    }

    assert!(
        msg.is_empty(),
        "Phase 275 W6 example fixture-coverage gate ({} leaves scanned, {} \
         covered dirs, {} tracked exceptions):{}",
        leaves.len(),
        covered.len(),
        allow.len(),
        msg,
    );
}
