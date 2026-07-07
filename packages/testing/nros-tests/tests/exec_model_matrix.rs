//! Phase 281 W5 — "no silent caps" gate for the RFC-0015 Model 1 execution-model
//! convergence matrix (language × platform).
//!
//! Phase-274 made Model 1 (one executor per priority tier over one shared
//! session, `active_groups`-gated) the single execution model for all
//! languages. Phase-281 closes the remaining lang×platform cells. This gate
//! keeps that matrix honest: every {language, platform} tier cell must be
//! EITHER
//!   - **covered** by a named end-to-end tiers test that exists in `tests/`, OR
//!   - **deferred** with an explicit reason + the work item that closes it.
//!
//! A cell that is neither — or a "covered" cell whose test file was removed /
//! renamed — fails this test. That closes the loophole where a matrix cell
//! silently regresses to unproven while the docs/artifact still read "done".
//!
//! Scope: the *tier* (`run_tiers` / Model 1) cell only — single-tier
//! `run_entry` boards (threadx, and the single-tier fallbacks) are a different
//! path and out of scope here. When a deferred cell lands its test, move it
//! from `DEFERRED` to `COVERED` in the SAME change that adds the test.

use std::path::PathBuf;

/// The languages Model 1 must reach.
const LANGS: &[&str] = &["rust", "c", "cpp"];
/// The platforms with a multi-tier (`run_tiers`) path in scope.
const PLATFORMS: &[&str] = &["native", "freertos", "zephyr", "nuttx"];

/// Cells proven by a named e2e. The `&str` is the test file (without `.rs`) in
/// `packages/testing/nros-tests/tests/`; the gate asserts it exists.
const COVERED: &[(&str, &str, &str)] = &[
    // (lang, platform, covering test file)
    ("rust", "native", "realtime_tiers_e2e"),
    ("rust", "freertos", "orchestration_tiers_freertos"),
    ("rust", "zephyr", "realtime_tiers_zephyr_entry_e2e"),
    ("c", "native", "realtime_tiers_c_e2e"),
    ("c", "freertos", "realtime_tiers_c_freertos_e2e"),
    ("cpp", "native", "realtime_tiers_cpp_e2e"),
    ("cpp", "freertos", "realtime_tiers_cpp_freertos_e2e"),
    ("cpp", "zephyr", "realtime_tiers_cpp_zephyr_e2e"),
    ("c", "zephyr", "realtime_tiers_c_zephyr_e2e"),
    ("cpp", "nuttx", "realtime_tiers_cpp_nuttx_e2e"),
    ("c", "nuttx", "realtime_tiers_c_nuttx_e2e"),
];

/// Cells not yet proven, each with a reason + the work item that closes it.
/// A deferred cell must NOT also be COVERED (checked below).
const DEFERRED: &[(&str, &str, &str)] = &[
    // (lang, platform, reason)
    (
        "rust",
        "nuttx",
        "phase-280 — nuttx entry runtime: eth0 IP push before Executor::open \
         (entry link landed #127; networking gated)",
    ),
];

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// Every cell of LANGS × PLATFORMS must appear exactly once across COVERED +
/// DEFERRED — no cell forgotten, no cell double-classified.
#[test]
fn every_matrix_cell_is_covered_or_explicitly_deferred() {
    let mut classified: std::collections::HashMap<(&str, &str), &str> =
        std::collections::HashMap::new();
    for (lang, plat, _) in COVERED {
        assert!(
            classified.insert((lang, plat), "covered").is_none(),
            "cell ({lang}, {plat}) appears more than once in COVERED",
        );
    }
    for (lang, plat, _) in DEFERRED {
        match classified.insert((lang, plat), "deferred") {
            None => {}
            Some(prev) => panic!(
                "cell ({lang}, {plat}) is in DEFERRED but already classified as {prev} \
                 (a cell must be covered XOR deferred, not both)",
            ),
        }
    }

    let mut missing = Vec::new();
    for lang in LANGS {
        for plat in PLATFORMS {
            if !classified.contains_key(&(*lang, *plat)) {
                missing.push(format!("({lang}, {plat})"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "execution-model matrix has unclassified cells (neither covered nor deferred) — \
         add a COVERED test or a DEFERRED reason for each:\n  {}",
        missing.join("\n  "),
    );

    // No stray cells outside the declared axes (typo guard).
    for (lang, plat) in classified.keys() {
        assert!(
            LANGS.contains(lang) && PLATFORMS.contains(plat),
            "cell ({lang}, {plat}) is outside the declared LANGS × PLATFORMS axes",
        );
    }
}

/// Every COVERED cell's e2e test file must exist — a removed/renamed tiers test
/// fails the gate instead of silently dropping the cell to unproven.
#[test]
fn covered_cells_have_their_e2e_test_file() {
    let dir = tests_dir();
    let mut absent = Vec::new();
    for (lang, plat, test) in COVERED {
        let path = dir.join(format!("{test}.rs"));
        if !path.is_file() {
            absent.push(format!("({lang}, {plat}) → {test}.rs"));
        }
    }
    assert!(
        absent.is_empty(),
        "COVERED cells name a tiers e2e that no longer exists (renamed/removed?) — \
         restore the test or move the cell to DEFERRED:\n  {}",
        absent.join("\n  "),
    );
}
