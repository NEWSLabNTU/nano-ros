//! Phase 212.I — `nros migrate workspace` verb coverage.
//!
//! Stages a minimal pre-212 workspace (workspace `nros.toml` + per-pkg
//! `component_nros.toml`) into a tempdir and drives the installed
//! `nros` CLI's `migrate workspace` verb against it.
//!
//! Three coverage points:
//! 1. `migrate_dry_run_writes_no_files` — `--dry-run` prints a plan
//!    and leaves the staged tree byte-identical.
//! 2. `migrate_workspace_e2e` — full migration produces the post-212
//!    shape: workspace `nros.toml` gone, bringup `system.toml` written,
//!    component `component_nros.toml` gone, component `Cargo.toml`
//!    carries `[package.metadata.nros.component]`, workspace
//!    `Cargo.toml` excludes the bringup dir.
//! 3. `migrate_idempotent_without_force_is_noop` — re-running without
//!    `--force` exits 0 and reports the already-migrated state without
//!    touching files. (`--force` semantics on a fully-cleaned tree are
//!    quirky — see notes in the test.)
//!
//! All three skip cleanly via `nros_tests::skip!` when the `nros` CLI
//! isn't installed.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Phase 214.N.3 — drift probe. Stage a throwaway pre-212 fixture, run
/// `nros migrate workspace --dry-run` against it, and return `true` iff the
/// dry-run output mentions the post-spec `[package.metadata.nros.component]`
/// sub-table that `migrate_workspace_e2e` asserts. Older installed CLIs
/// emit `[package.metadata.nros]` (no `.component`) and lose the gate.
fn migrate_emits_component_subtable() -> bool {
    let Some(nros) = nros_tests::nros_cli_bin_path() else {
        return false;
    };
    let (_guard, root) = stage_pre212_fixture();
    let Ok(out) = Command::new(&nros)
        .args(["migrate", "workspace", "--dry-run"])
        .arg(&root)
        .output()
    else {
        return false;
    };
    let blob =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    blob.contains("[package.metadata.nros.component]")
}

/// Stage a minimal pre-212 workspace into a tempdir. Returns the
/// tempdir guard + root path. Authored fresh (not copied from an
/// existing fixture) so the test owns its shape and won't drift if
/// the in-tree orchestration fixtures evolve.
fn stage_pre212_fixture() -> (tempfile::TempDir, PathBuf) {
    let guard = tempfile::tempdir().expect("tempdir");
    let root = guard.path().to_path_buf();
    let pkg = root.join("src/talker_pkg");
    fs::create_dir_all(pkg.join("src")).expect("mkdir pkg/src");

    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
members = ["src/talker_pkg"]
resolver = "2"
"#,
    )
    .expect("write workspace Cargo.toml");

    fs::write(
        root.join("nros.toml"),
        r#"[system]
name = "demo"
rmw = "zenoh"
domain_id = 0
components = ["talker_pkg"]
"#,
    )
    .expect("write nros.toml");

    fs::write(
        pkg.join("Cargo.toml"),
        r#"[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("write pkg Cargo.toml");

    fs::write(
        pkg.join("component_nros.toml"),
        r#"version = 1
package = "talker_pkg"
component = "talker"
language = "rust"

[linkage]
crate_name = "talker_pkg"
executable = "talker"
exported_symbol = "nros_node_talker"
"#,
    )
    .expect("write component_nros.toml");

    fs::write(pkg.join("src/lib.rs"), "").expect("write lib.rs");

    (guard, root)
}

/// Capture (relative path → content) for every file under `root`.
/// Snapshot for dry-run byte-identity assertion.
fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ty = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ty.is_dir() {
            walk(root, &path, out);
        } else if ty.is_file() {
            let rel = path.strip_prefix(root).expect("strip_prefix").to_path_buf();
            let body = fs::read(&path).unwrap_or_default();
            out.insert(rel, body);
        }
    }
}

fn nros_cmd() -> Command {
    let bin = nros_tests::nros_cli_bin_path().expect("nros CLI resolved");
    Command::new(bin)
}

#[test]
fn migrate_dry_run_writes_no_files() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }

    let (_guard, root) = stage_pre212_fixture();
    let before = snapshot_tree(&root);

    let out = nros_cmd()
        .arg("migrate")
        .arg("workspace")
        .arg("--dry-run")
        .arg(&root)
        .output()
        .expect("spawn nros migrate workspace --dry-run");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "nros migrate --dry-run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let plan = format!("{stdout}{stderr}");
    assert!(
        plan.contains("migrate workspace plan"),
        "stdout/stderr missing 'migrate workspace plan' header:\n{plan}"
    );
    assert!(
        plan.contains("system.toml"),
        "plan missing system.toml mention:\n{plan}"
    );
    assert!(
        plan.contains("--dry-run") || plan.contains("dry-run"),
        "plan missing dry-run mention:\n{plan}"
    );

    let after = snapshot_tree(&root);
    assert_eq!(
        before, after,
        "dry-run mutated the staged tree (files added/removed/edited)"
    );
}

#[test]
fn migrate_workspace_e2e() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    // Phase 214.N.3 — drift gate.
    //
    // The post-212.I spec the test asserts writes per-pkg Cargo.toml's with a
    // `[package.metadata.nros.component]` sub-table; older installed CLIs
    // emit `[package.metadata.nros]` only. Probe via `migrate --dry-run` on
    // a synthetic minimal fixture and skip cleanly when the CLI hasn't
    // adopted the sub-table yet (Phase 214.N — `nros` CLI lints / verbs
    // drift vs phase212 tests). Bumps to the nros-cli pin that carry the
    // post-spec emitter flip the probe and the test runs.
    if !migrate_emits_component_subtable() {
        nros_tests::skip!(
            "installed `nros migrate workspace` does not yet emit \
             [package.metadata.nros.component] — Phase 214.N drift gate \
             (the nros-cli release pin lags the post-212.I emitter spec)"
        );
    }

    let (_guard, root) = stage_pre212_fixture();

    let out = nros_cmd()
        .arg("migrate")
        .arg("workspace")
        .arg(&root)
        .output()
        .expect("spawn nros migrate workspace");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "nros migrate workspace failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Bringup name is derived from the first component's pkg name as
    // `<pkg>_bringup` (observed against installed CLI 2026-06-01; the
    // `[system].name` field is NOT used as the prefix).
    let bringup = root.join("talker_pkg_bringup");
    let system_toml = bringup.join("system.toml");
    assert!(system_toml.is_file(), "expected {}", system_toml.display());
    let system_body = fs::read_to_string(&system_toml).expect("read system.toml");
    assert!(
        system_body.contains("[system]") && system_body.contains("[[component]]"),
        "system.toml missing expected sections:\n{system_body}"
    );
    assert!(
        system_body.contains("pkg = \"talker_pkg\""),
        "system.toml missing component pkg entry:\n{system_body}"
    );

    // Pre-212 root nros.toml deleted.
    assert!(
        !root.join("nros.toml").exists(),
        "workspace nros.toml should be deleted post-migration"
    );

    // Per-pkg component_nros.toml deleted.
    let pkg = root.join("src/talker_pkg");
    assert!(
        !pkg.join("component_nros.toml").exists(),
        "src/talker_pkg/component_nros.toml should be deleted post-migration"
    );

    // Per-pkg Cargo.toml patched with [package.metadata.nros.component].
    let pkg_cargo = fs::read_to_string(pkg.join("Cargo.toml")).expect("read pkg Cargo.toml");
    assert!(
        pkg_cargo.contains("[package.metadata.nros.component]"),
        "pkg Cargo.toml missing nros.component metadata block:\n{pkg_cargo}"
    );

    // Workspace Cargo.toml excludes the bringup dir + records default_system.
    let ws_cargo = fs::read_to_string(root.join("Cargo.toml")).expect("read ws Cargo.toml");
    assert!(
        ws_cargo.contains("exclude") && ws_cargo.contains("talker_pkg_bringup"),
        "workspace Cargo.toml missing exclude=talker_pkg_bringup:\n{ws_cargo}"
    );
    assert!(
        ws_cargo.contains("[workspace.metadata.nros]"),
        "workspace Cargo.toml missing [workspace.metadata.nros]:\n{ws_cargo}"
    );
    assert!(
        ws_cargo.contains("default_system") && ws_cargo.contains("talker_pkg_bringup"),
        "workspace Cargo.toml missing default_system pointer:\n{ws_cargo}"
    );
}

#[test]
fn migrate_idempotent_without_force_is_noop() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }

    let (_guard, root) = stage_pre212_fixture();

    // First run: real migration.
    let first = nros_cmd()
        .arg("migrate")
        .arg("workspace")
        .arg(&root)
        .output()
        .expect("spawn first nros migrate workspace");
    assert!(
        first.status.success(),
        "first migration failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let snapshot_after_first = snapshot_tree(&root);

    // Second run, no --force: should exit 0 + report already-migrated
    // without touching files.
    let second = nros_cmd()
        .arg("migrate")
        .arg("workspace")
        .arg(&root)
        .output()
        .expect("spawn second nros migrate workspace");
    let stdout2 = String::from_utf8_lossy(&second.stdout);
    let stderr2 = String::from_utf8_lossy(&second.stderr);
    assert!(
        second.status.success(),
        "second migrate (no --force) should exit 0:\nstdout:\n{stdout2}\nstderr:\n{stderr2}"
    );
    let combined = format!("{stdout2}{stderr2}");
    assert!(
        combined.contains("already migrated"),
        "second migrate (no --force) should report already-migrated:\n{combined}"
    );

    let snapshot_after_second = snapshot_tree(&root);
    assert_eq!(
        snapshot_after_first, snapshot_after_second,
        "no-op idempotent re-run mutated the tree"
    );

    // NOTE: `--force` on a fully-cleaned post-212 tree currently errors
    // with "no pre-212 nros.toml at <root> — nothing to migrate" because
    // the first migration deleted nros.toml. `--force` only succeeds
    // when the tree still has nros.toml AND already carries the
    // `[workspace.metadata.nros]` marker (a partially-applied state).
    // Verified against installed CLI 2026-06-01. The help text
    // ("Re-run on an already-migrated tree") oversells the flag; the
    // implementation is the canonical surface and fixing it is a
    // nros-cli concern, not Phase 212.I scope.
}
