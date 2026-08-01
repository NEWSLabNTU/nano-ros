//! phase-296 R-code — `nros plan` consumes a COMMITTED SystemModel; the
//! launch-XML parse/synthesis path was removed (plan.rs R-code.1).
//!
//! This file used to assert the three branches of the pre-296
//! `launch_synth` resolution policy (in-memory `<launch>` synthesis for a
//! single-bin pkg, `<pkg>.launch.xml` vs `system.launch.xml` precedence,
//! Path-A no-synth refusal) by driving `nros plan <pkg> <dir>`. Two of
//! those behaviors — synthesis and launch-file precedence — now live in
//! `ros-launch-resolve` (RFC-0060 layer 2) and are tested in its own repo;
//! `nros plan` never parses launch XML any more, so those two tests could
//! only assert deleted behavior and were removed (issue 0381).
//!
//! What remains here is the one branch still owned by `nros plan`: a
//! bringup that carries no committed SystemModel is refused with a clear
//! error. This test needs only the `nros` CLI — no `play_launch_parser`.

use std::{fs, path::Path, process::Command};

/// Stage a Path A bringup pkg fixture (NO Cargo.toml / CMakeLists.txt, and
/// crucially NO committed `config/system_model.yaml`). The presence of
/// system.toml + package.xml without a resolved model is what `nros plan`
/// must refuse post-296.
fn stage_path_a_bringup_no_launch(root: &Path) {
    fs::create_dir_all(root.join("bringup_pkg")).unwrap();
    fs::write(
        root.join("bringup_pkg/package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>bringup_pkg</name>
  <version>0.1.0</version>
  <description>Path A bringup no-model failure fixture.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <export><build_type>ament_nros</build_type></export>
</package>
"#,
    )
    .unwrap();
    fs::write(
        root.join("bringup_pkg/system.toml"),
        r#"[system]
name = "alpha"
rmw = "zenoh"
domain_id = 0
"#,
    )
    .unwrap();
}

/// A bringup with no committed SystemModel is refused, with the current
/// phase-296 error contract (not the pre-296 "synthesis is disallowed"
/// wording — that path is deleted).
#[test]
fn nros_plan_refuses_bringup_with_no_committed_model() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    }
    let td = tempfile::tempdir().expect("tempdir");
    stage_path_a_bringup_no_launch(td.path());

    let nros = nros_tests::nros_cli_bin_path().expect("nros bin");
    let out_dir = td.path().join("out");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("bringup_pkg")
        .arg("bringup_pkg")
        .arg("--workspace")
        .arg(td.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .current_dir(td.path())
        .output()
        .expect("spawn nros plan");
    assert!(
        !result.status.success(),
        "nros plan should fail for a bringup with no committed SystemModel\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    // The current (phase-296 R-code.1) contract: no committed model + the
    // launch-XML parse path removed.
    assert!(
        stderr.contains("no committed SystemModel") || stderr.contains("launch-XML parse path"),
        "stderr should mention the missing committed SystemModel contract:\n{stderr}"
    );
}
