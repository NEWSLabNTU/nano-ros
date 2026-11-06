//! Phase 212.L.7 — `nros plan <pkg-dir>` on a package with no launch file.
//!
//! These tests drive the `nros` CLI against staged tempdir fixtures:
//!
//! 1. `nros_plan_self_bringup_emits_plan_json` — a single-pkg dir w/
//!    `[package.metadata.nros.component]` and no `launch/` dir is planned as
//!    a synthesised 1-node model, and `nros plan` writes `record.json`.
//! 2. `nros_plan_self_bringup_uses_launch_synth_when_absent` — confirm the
//!    synth fires for such a pkg rather than a Path A no-launch error.
//!
//! The `[package.metadata.nros.deploy.*]` half of the L.7 surface (a package
//! baked as its own bringup by `nros codegen-system`) retired in phase-531 W7
//! step 1; the CLI refuses the table by name.

use std::{fs, path::Path, process::Command};

// issue 0381 — the pre-296 ~~`play_launch_parser_available()`~~ gate is GONE:
// `nros plan`'s self-bringup path synthesises the model in-process
// (plan.rs `synthesise_self_model`) and never shells to the parser, so a
// parser-availability skip only hid these tests on every host without it.
fn require_nros_cli_only() {
    nros_tests::require_nros_cli();
}

/// Stage a launchless component pkg fixture: a single workspace member
/// whose `Cargo.toml` carries `[package.metadata.nros.component]` and ships a
/// sibling `package.xml`. No `launch/` dir — the L.6 synth fires.
fn stage_self_bringup_component_pkg(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["alpha_pkg"]

[workspace.metadata.nros]
default_system = "alpha_pkg"
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("alpha_pkg/src")).unwrap();
    fs::write(
        root.join("alpha_pkg/Cargo.toml"),
        r#"[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "alpha_pkg"
path = "src/main.rs"

[package.metadata.nros.component]
class = "alpha_pkg::Node"
name = "alpha"
default_namespace = "/"
"#,
    )
    .unwrap();
    fs::write(root.join("alpha_pkg/src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        root.join("alpha_pkg/package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>alpha_pkg</name>
  <version>0.1.0</version>
  <description>L.7 self-bringup component fixture.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <export><build_type>ament_nros</build_type></export>
</package>
"#,
    )
    .unwrap();
}

/// `nros plan <pkg-dir>` for a self-bringup pkg writes a `record.json`
/// derived from the synth XML.
#[test]
fn nros_plan_self_bringup_emits_plan_json() {
    require_nros_cli_only();
    let td = tempfile::tempdir().expect("tempdir");
    stage_self_bringup_component_pkg(td.path());

    let nros = nros_tests::nros_cli_bin_path().expect("nros bin");
    let out_dir = td.path().join("out");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("alpha_pkg") // system_pkg name
        .arg("alpha_pkg") // pkg dir → resolver / synth
        .arg("--workspace")
        .arg(td.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .current_dir(td.path())
        .output()
        .expect("spawn nros plan");
    // Full planning success requires source-metadata; we only assert the
    // resolver-output (`record.json`) was written — same convention as
    // ~~`phase212_l6_launch_synth`~~.
    let record_path = out_dir.join("record.json");
    assert!(
        record_path.is_file(),
        "record.json missing at {} (synth did not reach the parser)\nstdout:\n{}\nstderr:\n{}",
        record_path.display(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    let record_text = fs::read_to_string(&record_path).unwrap();
    // Synth XML names the pkg as both `pkg` + `exec`.
    assert!(
        record_text.contains("alpha_pkg"),
        "record.json missing synth-derived pkg `alpha_pkg`:\n{record_text}"
    );
}

/// Confirm the L.6 synth resolver fires when the pkg has no
/// `launch/` dir — i.e. the record carries the synth pkg/exec attrs
/// rather than a Path A no-launch error.
#[test]
fn nros_plan_self_bringup_uses_launch_synth_when_absent() {
    require_nros_cli_only();
    let td = tempfile::tempdir().expect("tempdir");
    stage_self_bringup_component_pkg(td.path());
    // Sanity: no launch dir exists.
    assert!(!td.path().join("alpha_pkg/launch").exists());

    let nros = nros_tests::nros_cli_bin_path().expect("nros bin");
    let out_dir = td.path().join("out");
    let _ = Command::new(&nros)
        .arg("plan")
        .arg("alpha_pkg")
        .arg("alpha_pkg")
        .arg("--workspace")
        .arg(td.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .current_dir(td.path())
        .output()
        .expect("spawn nros plan");
    let record = fs::read_to_string(out_dir.join("record.json")).expect("record.json");
    // Synth's <launch><node pkg="alpha_pkg" exec="alpha_pkg" /></launch>
    // pipes through the parser; the record names both as `alpha_pkg`.
    assert!(
        record.contains("alpha_pkg"),
        "record.json missing synth-derived attrs:\n{record}"
    );
}
