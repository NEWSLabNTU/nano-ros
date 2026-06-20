//! Phase 212.M.11 — canonical-shape regression walker for `examples/`.
//!
//! Phase 212 is a clean break from the pre-212 `nros.toml` +
//! `component_nros.toml` + `gen-app-config.py` + `app_config.h.in` +
//! committed `metadata/*.json` shape. The 2026-06-02 §212.L redesign
//! locks the canonical Node pkg + Entry pkg taxonomy (the
//! Bringup pkg is RETIRED, subsumed by Entry pkg — see
//! `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
//! §212.L + §212.N).
//!
//! This walker locks the example tree against pre-212 regression so
//! once the §212.M sweep completes (M.5.b shipped, M.7 in-progress,
//! M.10 pending) the new shape is the only shape that compiles.
//!
//! Carve-outs (per CLAUDE.md Examples = Standalone Projects):
//! - `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` — one-board-one-RMW
//!   reference, not collapsed
//! - `examples/bridges/` — cross-RMW gateways with their own conventions
//! - `examples/templates/` — multi-platform copy-out recipes (Pattern A
//!   workspaces etc.) — fixture-shaped, not canonical-shape
//!
//! Test-fixture binaries live OUT of `examples/` (per the same CLAUDE.md
//! section) — under `packages/testing/nros-tests/fixtures/` — so the
//! `tests/fixtures/` carve-out for `[package.metadata.nros.*]`-absent
//! Cargo.toml does not fire in practice; it is kept for forward-compat.
//!
//! Pure file-walk + TOML parse. No SDK / toolchain dependency.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// Directories under `examples/` that are skipped (top-level path
/// components, matched at the immediate child of `examples/`).
const SKIP_TOP_LEVEL: &[&str] = &["bridges", "templates"];

/// Specific example dirs that are not §212.L-shaped by design
/// (per CLAUDE.md carve-outs).
fn is_carve_out(rel: &Path) -> bool {
    // examples/zephyr/cpp/cyclonedds/talker-aemv8r/
    rel.starts_with("zephyr/cpp/cyclonedds/talker-aemv8r")
}

/// Directories never descended into.
fn skip_dir(name: &str) -> bool {
    if name.starts_with("build-") {
        return true;
    }
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

/// Recursive walk: emit every directory that contains `package.xml`.
fn collect_pkg_dirs(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, acc: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut has_pkg_xml = false;
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            let name = match p.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
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

#[derive(Debug)]
struct Violation {
    dir: PathBuf,
    reason: String,
}

/// Pre-212 file names that must never appear at the same level as a
/// `package.xml`.
const FORBIDDEN_FILES: &[&str] = &[
    "nros.toml",
    "component_nros.toml",
    "gen-app-config.py",
    "app_config.h.in",
];

/// Check one pkg dir for canonical-shape violations.
fn check_pkg_dir(examples_root: &Path, pkg_dir: &Path, violations: &mut Vec<Violation>) {
    let rel = pkg_dir
        .strip_prefix(examples_root)
        .unwrap_or(pkg_dir)
        .to_path_buf();

    // 1. Pre-212 sidecar files must not be present at this level.
    for fname in FORBIDDEN_FILES {
        if pkg_dir.join(fname).is_file() {
            violations.push(Violation {
                dir: rel.clone(),
                reason: format!("pre-212 file `{fname}` present at package level"),
            });
        }
    }

    // 2. Committed `metadata/*.json` build artifacts.
    let metadata_dir = pkg_dir.join("metadata");
    if metadata_dir.is_dir()
        && let Ok(entries) = fs::read_dir(&metadata_dir)
    {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("json") {
                violations.push(Violation {
                    dir: rel.clone(),
                    reason: format!(
                        "committed build artifact `metadata/{}` (must not be \
                             tracked; lives in $OUT_DIR/nros-gen/ or target/nros-metadata/)",
                        p.file_name().and_then(|s| s.to_str()).unwrap_or("?")
                    ),
                });
            }
        }
    }

    // 3. Cargo.toml metadata taxonomy.
    let cargo_toml = pkg_dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return;
    }
    let text = match fs::read_to_string(&cargo_toml) {
        Ok(t) => t,
        Err(e) => {
            violations.push(Violation {
                dir: rel.clone(),
                reason: format!("Cargo.toml unreadable: {e}"),
            });
            return;
        }
    };
    // toml 0.9: the `FromStr` impl on `toml::Value` is value-shaped
    // (rejects top-level tables); use `toml::from_str` for full Cargo
    // documents. See packages/testing/nros-tests/tests/phase212_m12_
    // example_shape.rs for the sibling regression test that surfaced
    // the same bug.
    let value: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            violations.push(Violation {
                dir: rel,
                reason: format!("Cargo.toml parse error: {e}"),
            });
            return;
        }
    };

    let pkg_name = value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let nros_meta = value
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"));

    // tests/fixtures/ carve-out: a Cargo.toml may opt out of metadata.nros
    // entirely if it lives under `tests/fixtures/`.
    let is_fixture = rel
        .components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "fixtures");

    let nros_meta = match nros_meta {
        Some(m) => m,
        None => {
            if !is_fixture {
                violations.push(Violation {
                    dir: rel,
                    reason: "Cargo.toml missing `[package.metadata.nros.{component,entry,\
                             application}]` table (and not a tests/fixtures/ opt-out)"
                        .into(),
                });
            }
            return;
        }
    };

    // Phase 212.N.12 — `node` is the canonical spelling for the
    // single-shape Node pkg surface (renamed from `component`); the
    // sibling `phase212_m12_example_shape.rs::component_or_application_
    // classification_present` test accepts the same alias set. The
    // pre-N.12 `component` spelling is kept for back-compat. See
    // `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
    // §212.N (rename wave — `[package.metadata.nros.component]` →
    // `[package.metadata.nros.node]` landed in 9bef3ff0c).
    let has_component = nros_meta.get("component").is_some();
    let has_node = nros_meta.get("node").is_some();
    let has_entry = nros_meta.get("entry").is_some();
    // Pre-rename `application` is still accepted by the §212.M sweep
    // (renamed to `entry` per §212.N.5; both shapes valid until M.10
    // completes).
    let has_application = nros_meta.get("application").is_some();

    if !(has_component || has_node || has_entry || has_application) {
        violations.push(Violation {
            dir: rel.clone(),
            reason: "Cargo.toml carries `[package.metadata.nros]` but lacks `component` / \
                     `node` / `entry` / `application` subtable (§212.L canonical shapes \
                     + §212.N.12 rename)"
                .into(),
        });
        return;
    }

    // §212.L.4 — `class = "<pkg>::<Class>"` prefix rule.
    // Applies to both `[...component]` (pre-N.12) and `[...node]` (post-N.12).
    if has_component || has_node {
        let component = nros_meta
            .get("node")
            .or_else(|| nros_meta.get("component"))
            .unwrap();
        let class = component.get("class").and_then(|v| v.as_str());
        match (class, pkg_name.as_deref()) {
            (Some(c), Some(p)) => {
                let prefix = format!("{p}::");
                if !c.starts_with(&prefix) {
                    violations.push(Violation {
                        dir: rel,
                        reason: format!(
                            "[package.metadata.nros.component] class = \"{c}\" must start \
                             with package name prefix \"{prefix}\" (§212.L.4)"
                        ),
                    });
                }
            }
            (None, _) => {
                violations.push(Violation {
                    dir: rel,
                    reason: "[package.metadata.nros.component] missing `class` field".into(),
                });
            }
            (Some(_), None) => {
                violations.push(Violation {
                    dir: rel,
                    reason: "[package.metadata.nros.component] present but Cargo.toml has \
                             no [package].name"
                        .into(),
                });
            }
        }
    }
}

#[test]
fn examples_tree_uses_canonical_shape() {
    let root = nros_tests::project_root();
    let examples_root = root.join("examples");
    if !examples_root.is_dir() {
        nros_tests::skip!(
            "examples/ directory missing at {} — wrong project_root?",
            examples_root.display()
        );
    }

    // Collect candidate pkg dirs, skipping the carve-out top-levels.
    let mut all = Vec::new();
    let entries = fs::read_dir(&examples_root).expect("read examples/");
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = match p.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if SKIP_TOP_LEVEL.contains(&name) {
            continue;
        }
        all.extend(collect_pkg_dirs(&p));
    }

    let mut violations: Vec<Violation> = Vec::new();
    for pkg in &all {
        let rel = pkg.strip_prefix(&examples_root).unwrap_or(pkg);
        if is_carve_out(rel) {
            continue;
        }
        check_pkg_dir(&examples_root, pkg, &mut violations);
    }

    if !violations.is_empty() {
        // Dedup by (dir, reason).
        let mut seen = BTreeSet::new();
        let mut lines = Vec::with_capacity(violations.len());
        for v in &violations {
            let key = format!("{}|{}", v.dir.display(), v.reason);
            if seen.insert(key) {
                lines.push(format!("  - {}: {}", v.dir.display(), v.reason));
            }
        }
        // Sort for stable output.
        lines.sort();
        let flagged_dirs: BTreeSet<_> = violations.iter().map(|v| v.dir.clone()).collect();
        panic!(
            "Phase 212.M.11 canonical-shape regression: {} violation(s) across {} \
             pkg dir(s) (out of {} package.xml-bearing dirs scanned).\n\n\
             Punch list (one line per violation, deduped):\n{}\n\n\
             Expected shape (§212.L + §212.N):\n  \
             - No pre-212 sidecar files (`nros.toml`, `component_nros.toml`, \
             `gen-app-config.py`, `app_config.h.in`)\n  \
             - No committed `metadata/*.json` (build artifact only; lives in \
             $OUT_DIR/nros-gen/ or target/nros-metadata/)\n  \
             - Cargo.toml carries `[package.metadata.nros.{{component,node,entry,\
             application}}]`, and for `component`/`node`: `class = \"<pkg-name>::<Class>\"`",
            violations.len(),
            flagged_dirs.len(),
            all.len(),
            lines.join("\n"),
        );
    }
}
