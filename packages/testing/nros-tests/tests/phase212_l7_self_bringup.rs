//! Phase 212.L.7 — self-bringup component / application pkg integration.
//!
//! These tests drive the `nros` CLI against staged tempdir fixtures and
//! assert the three pieces of the L.7 surface:
//!
//! 1. `nros_plan_self_bringup_emits_plan_json` — a single-pkg dir w/
//!    `[package.metadata.nros.component]` + `[package.metadata.nros.deploy.native]`
//!    is treated as its own 1-component bringup. `nros plan` resolves the
//!    launch via the L.6 synth path and writes `record.json`
//!    (sufficient proof: planner consumed the synth XML).
//! 2. `nros_codegen_system_self_bringup_bakes_system_main` — `nros
//!    codegen-system --bringup <pkg-dir>` produces `system_config.h`
//!    + `system_main.c` populated from the pkg's deploy block + class.
//! 3. `nros_plan_self_bringup_uses_launch_synth_when_absent` — confirm
//!    L.6 synth fires for an L.7 self-bringup pkg w/ no `launch/` dir.

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

fn play_launch_parser_available() -> bool {
    Command::new("play_launch_parser")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn require_nros_cli_only() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    }
}

fn require_tools() {
    require_nros_cli_only();
    if !play_launch_parser_available() {
        nros_tests::skip!(
            "play_launch_parser not on PATH (pip install play-launch-parser, or build its binary)"
        );
    }
}

/// Stage a self-bringup component pkg fixture: a single workspace member
/// whose `Cargo.toml` carries `[package.metadata.nros.component]` +
/// `[package.metadata.nros.deploy.native]` and ships a sibling
/// `package.xml`. No `launch/` dir — the L.6 synth fires.
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

[package.metadata.nros.deploy.native]
board = "native_sim/native/64"
rmw = "zenoh"
domain_id = 7
locator = "tcp/127.0.0.1:7447"
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
    require_tools();
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
    // `phase212_l6_launch_synth`.
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

/// `nros codegen-system --bringup <pkg-dir>` bakes a 1-component
/// `system_main.c` + `system_config.h` from the pkg's L.7 metadata.
#[test]
fn nros_codegen_system_self_bringup_bakes_system_main() {
    require_nros_cli_only();
    let td = tempfile::tempdir().expect("tempdir");
    stage_self_bringup_component_pkg(td.path());

    let nros = nros_tests::nros_cli_bin_path().expect("nros bin");
    let out_dir = td.path().join("out");
    let result = Command::new(&nros)
        .arg("codegen-system")
        .arg("--workspace")
        .arg(td.path())
        .arg("--bringup")
        .arg("alpha_pkg")
        .arg("--target")
        .arg("native")
        .arg("--out")
        .arg(&out_dir)
        .current_dir(td.path())
        .output()
        .expect("spawn nros codegen-system");
    assert!(
        result.status.success(),
        "nros codegen-system failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );

    let bake = out_dir.join("nros-system");
    let header = fs::read_to_string(bake.join("system_config.h")).expect("system_config.h");
    assert!(
        header.contains("#define NROS_SYSTEM_DOMAIN_ID 7u"),
        "header missing domain_id bake:\n{header}"
    );
    assert!(
        header.contains("#define NROS_SYSTEM_RMW \"zenoh\""),
        "header missing rmw bake:\n{header}"
    );
    assert!(
        header.contains("#define NROS_SYSTEM_LOCATOR \"tcp/127.0.0.1:7447\""),
        "header missing locator bake:\n{header}"
    );
    assert!(
        header.contains("#define NROS_SYSTEM_COMPONENT_COUNT 1"),
        "header missing component count:\n{header}"
    );
    assert!(
        header.contains("#define NROS_SYSTEM_COMPONENT_0_NAME \"alpha\""),
        "header missing component name:\n{header}"
    );
    assert!(
        header.contains("#define NROS_SYSTEM_COMPONENT_0_CLASS \"alpha_pkg::Node\""),
        "header missing component class:\n{header}"
    );

    let main_c = fs::read_to_string(bake.join("system_main.c")).expect("system_main.c");
    assert!(
        main_c.contains("nros_component_alpha_register"),
        "system_main.c missing component register call:\n{main_c}"
    );

    let plan = fs::read_to_string(bake.join("nros-plan.json")).expect("nros-plan.json");
    assert!(plan.contains("\"bringup\": \"alpha_pkg\""), "plan: {plan}");
    assert!(plan.contains("\"system\": \"alpha_pkg\""), "plan: {plan}");
    assert!(
        plan.contains("\"class\": \"alpha_pkg::Node\""),
        "plan: {plan}"
    );
}

/// Confirm the L.6 synth resolver fires when the L.7 pkg has no
/// `launch/` dir — i.e. the record carries the synth pkg/exec attrs
/// rather than a Path A no-launch error.
#[test]
fn nros_plan_self_bringup_uses_launch_synth_when_absent() {
    require_tools();
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
