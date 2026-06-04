//! Phase 212.F — `nros new system` + `nros check --bringup` smoke.
//!
//! Two cases:
//! 1. `nros_new_system_scaffolds_bringup_pkg` — scaffold a bringup pkg in a
//!    minimal cargo workspace tempdir, assert the documented file tree
//!    (`package.xml`, `system.toml`, `launch/system.launch.xml`), assert
//!    `system.toml` carries `[[component]]` blocks for each component
//!    passed via `--components`, and assert the workspace `Cargo.toml`
//!    has the bringup dir added to `[workspace] exclude` (Path A: bringup
//!    is excluded from workspace members).
//! 2. `nros_check_rejects_cargo_toml_in_bringup` — drop a stray
//!    `Cargo.toml` next to `system.toml`, assert `nros check --bringup`
//!    exits non-zero AND stderr mentions `Cargo.toml`.
//!
//! Skips cleanly via `nros_tests::skip!` when the `nros` CLI isn't found.

use std::{fs, path::PathBuf, process::Command};

fn nros_bin() -> Option<PathBuf> {
    nros_tests::nros_cli_bin_path()
}

/// Initialise a tempdir with a minimal cargo workspace root and return
/// `(guard, root_path)`. Guard keeps the tempdir alive.
fn fresh_workspace() -> (tempfile::TempDir, PathBuf) {
    let td = tempfile::tempdir().expect("tempdir");
    let root = td.path().to_path_buf();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .expect("write workspace Cargo.toml");
    (td, root)
}

#[test]
fn nros_new_system_scaffolds_bringup_pkg() {
    let Some(nros) = nros_bin() else {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    };
    let (_guard, root) = fresh_workspace();

    let out = Command::new(&nros)
        .args([
            "new",
            "system",
            "demo_bringup",
            "--components",
            "talker_pkg,listener_pkg",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn nros new system");
    assert!(
        out.status.success(),
        "nros new system failed (exit={:?})\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let bringup = root.join("demo_bringup");
    for rel in ["package.xml", "system.toml", "launch/system.launch.xml"] {
        let p = bringup.join(rel);
        assert!(p.is_file(), "missing scaffolded file: {}", p.display());
    }

    let system_toml = fs::read_to_string(bringup.join("system.toml")).expect("read system.toml");
    assert!(
        system_toml.contains("[[component]]"),
        "system.toml missing [[component]] block(s):\n{system_toml}",
    );
    // Both component pkg names should appear under a `pkg = "..."` key.
    for comp in ["talker_pkg", "listener_pkg"] {
        let needle = format!("pkg = \"{comp}\"");
        assert!(
            system_toml.contains(&needle),
            "system.toml missing `{needle}`:\n{system_toml}",
        );
    }

    // Workspace Cargo.toml must add the bringup dir to [workspace] exclude
    // (Path A: bringup is NOT a workspace member).
    let ws_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read root Cargo.toml");
    let has_exclude = ws_toml
        .lines()
        .any(|l| l.trim_start().starts_with("exclude") && l.contains("demo_bringup"));
    assert!(
        has_exclude,
        "workspace Cargo.toml missing `exclude = [.. \"demo_bringup\" ..]`:\n{ws_toml}",
    );
}

#[test]
fn nros_check_rejects_cargo_toml_in_bringup() {
    let Some(nros) = nros_bin() else {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    };
    let (_guard, root) = fresh_workspace();

    // Scaffold first.
    let scaffold = Command::new(&nros)
        .args([
            "new",
            "system",
            "demo_bringup",
            "--components",
            "talker_pkg,listener_pkg",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn nros new system");
    assert!(
        scaffold.status.success(),
        "nros new system failed (exit={:?})\nstderr:\n{}",
        scaffold.status.code(),
        String::from_utf8_lossy(&scaffold.stderr),
    );

    // Drop a stray Cargo.toml next to system.toml — the lint should trip.
    let bringup = root.join("demo_bringup");
    fs::write(
        bringup.join("Cargo.toml"),
        "[package]\nname = \"demo_bringup\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write stray Cargo.toml");

    let out = Command::new(&nros)
        .args(["check", "--bringup", "demo_bringup"])
        .current_dir(&root)
        .output()
        .expect("spawn nros check --bringup");
    assert!(
        !out.status.success(),
        "nros check --bringup unexpectedly succeeded with stray Cargo.toml\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Cargo.toml"),
        "nros check stderr should mention `Cargo.toml`:\n{stderr}",
    );
}
