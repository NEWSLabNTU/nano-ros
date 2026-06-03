//! Phase 212.M.12 — pre-212 file ban (tree-wide grep).
//!
//! Companion to the §212.M.11 canonical-shape walker
//! (`phase212_examples_canonical_shape.rs`). Per the 2026-06-02 §212.L
//! redesign + §212.N follow-up, Phase 212 is a clean break from the
//! pre-212 surface — see
//! `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`.
//!
//! Hard-asserts that NO file matching any pre-212 name appears
//! ANYWHERE under `examples/` OR `packages/testing/nros-tests/fixtures/`.
//! Build artifacts (`target/`, `build/`, `.git/`, generated dirs) are
//! skipped so a stale local build does not flake the test.
//!
//! Pure file-walk. No SDK / toolchain dependency.

use std::{
    fs,
    path::{Path, PathBuf},
};

const FORBIDDEN_FILES: &[&str] = &[
    "nros.toml",
    "component_nros.toml",
    "gen-app-config.py",
    "app_config.h.in",
];

fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        "target"
            | "build"
            | ".git"
            | "node_modules"
            | "generated"
            | ".cargo"
            | "cmake-build-debug"
            | "cmake-build-release"
    )
}

fn walk(dir: &Path, hits: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = match p.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if p.is_dir() {
            if !skip_dir(name) {
                walk(&p, hits);
            }
        } else if FORBIDDEN_FILES.contains(&name) {
            hits.push(p.clone());
        } else if name.ends_with(".json") {
            // Committed `metadata/*.json` build artifacts.
            if let Some(parent) = p
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                && parent == "metadata"
            {
                hits.push(p);
            }
        }
    }
}

#[test]
fn examples_tree_has_no_pre_212_files() {
    let root = nros_tests::project_root();
    let examples_root = root.join("examples");
    if !examples_root.is_dir() {
        nros_tests::skip!(
            "examples/ directory missing at {} — wrong project_root?",
            examples_root.display()
        );
    }

    let mut hits = Vec::new();
    walk(&examples_root, &mut hits);
    hits.sort();
    if !hits.is_empty() {
        let lines: Vec<String> = hits
            .iter()
            .map(|p| format!("  - {}", p.strip_prefix(&root).unwrap_or(p).display()))
            .collect();
        panic!(
            "Phase 212.M.12: {} pre-212 file(s) found under examples/. \
             These shapes are RETIRED — see \
             docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md \
             §212.L + §212.M.\n\n{}",
            hits.len(),
            lines.join("\n"),
        );
    }
}

#[test]
fn nros_tests_fixtures_have_no_pre_212_files() {
    let root = nros_tests::project_root();
    let fixtures_root = root.join("packages/testing/nros-tests/fixtures");
    if !fixtures_root.is_dir() {
        nros_tests::skip!(
            "fixtures/ directory missing at {} — wrong project_root?",
            fixtures_root.display()
        );
    }

    let mut hits = Vec::new();
    walk(&fixtures_root, &mut hits);
    hits.sort();
    if !hits.is_empty() {
        let lines: Vec<String> = hits
            .iter()
            .map(|p| format!("  - {}", p.strip_prefix(&root).unwrap_or(p).display()))
            .collect();
        panic!(
            "Phase 212.M.12: {} pre-212 file(s) found under \
             packages/testing/nros-tests/fixtures/. These shapes are RETIRED — \
             fixtures must be migrated per §212.I.3.\n\n{}",
            hits.len(),
            lines.join("\n"),
        );
    }
}
