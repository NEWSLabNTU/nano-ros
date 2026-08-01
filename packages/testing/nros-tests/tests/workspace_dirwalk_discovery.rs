//! Phase 212.F.3 — Path A bringup discovery via dirwalk.
//!
//! Bringup packages ship `package.xml` + `system.toml` but NO `Cargo.toml`,
//! so cargo's workspace `members` list never sees them. Workspaces using the
//! canonical Path A shape add the bringup dir to `[workspace] exclude`, but
//! the planner finds the dir via a shallow dirwalk over `workspace_root`
//! either way — `exclude` is just hygiene, not load-bearing.
//!
//! These two tests stage a minimal cargo workspace tempdir with a sibling
//! bringup dir and assert `nros plan` discovers it.
//!
//! Phase 212.A `cargo-nros` cargo subcommand shell was retracted (the
//! cargo prefix added no functional value over the bare `nros` verb —
//! see phase doc §212.A); the dirwalk discovery surface IS `nros plan`.
//!
//! Skips cleanly via `nros_tests::skip!` when the `nros` CLI (built
//! in-tree at `packages/cli/target/release/nros` by `just setup-cli`;
//! Phase 218) cannot be resolved.

use std::{fs, path::Path, process::Command};

/// Stage a Path A workspace at `root` with the given top-level Cargo.toml.
/// The fixture is:
///
/// ```text
/// root/
///   Cargo.toml           # supplied verbatim
///   talker_pkg/
///     Cargo.toml         # member, carries [package.metadata.nros.component]
///     src/lib.rs         # empty
///   demo_bringup/
///     package.xml
///     system.toml
///     config/system_model.yaml   # committed resolved model (phase-296)
/// ```
fn stage_fixture(root: &Path, cargo_toml: &str) {
    fs::write(root.join("Cargo.toml"), cargo_toml).expect("write workspace Cargo.toml");

    fs::create_dir_all(root.join("talker_pkg/src")).expect("mkdir talker_pkg/src");
    fs::write(
        root.join("talker_pkg/Cargo.toml"),
        r#"[package]
name = "talker_pkg"
version = "0.0.1"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "talker_pkg::node"
name = "talker"
"#,
    )
    .expect("write talker_pkg Cargo.toml");
    fs::write(root.join("talker_pkg/src/lib.rs"), "").expect("write talker lib.rs");

    fs::create_dir_all(root.join("demo_bringup/config")).expect("mkdir demo_bringup/config");
    fs::write(
        root.join("demo_bringup/package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>demo_bringup</name>
  <version>0.0.0</version>
  <description>dirwalk discovery fixture</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <exec_depend>talker_pkg</exec_depend>
  <export><build_type>ament_cmake</build_type></export>
</package>
"#,
    )
    .expect("write demo_bringup/package.xml");
    fs::write(
        root.join("demo_bringup/system.toml"),
        r#"[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::node"
name = "talker"
"#,
    )
    .expect("write demo_bringup/system.toml");
    // phase-296 R-code: `nros plan` consumes a COMMITTED SystemModel, not a
    // launch file. The committed model is what dirwalk discovery must find
    // under the non-member bringup dir (issue 0381).
    fs::write(
        root.join("demo_bringup/config/system_model.yaml"),
        r#"meta:
  version: 1
structure:
  scopes:
    /: {}
  nodes:
    /talker:
      scope: /
      pkg: talker_pkg
      exec: talker
"#,
    )
    .expect("write demo_bringup/config/system_model.yaml");
}

/// Invoke `nros plan demo_bringup demo_bringup --workspace <root>` and assert
/// `nros plan` DISCOVERED the sibling bringup — i.e. it found and loaded
/// `demo_bringup/config/system_model.yaml` even though `demo_bringup` is not a
/// cargo workspace member (no Cargo.toml, invisible to `cargo metadata`).
///
/// We assert on the discovery signal, NOT full planning success: the committed
/// model names `talker_pkg/talker`, and completing the plan would need that
/// component's source-metadata (a build artifact this fixture doesn't stage),
/// so the planner fails in the metadata walk AFTER discovery. Discovery is what
/// this test owns; the metadata walk is a separate concern (same
/// output-before-metadata-walk rationale the resolver tests use).
fn run_plan_and_assert(root: &Path) {
    let nros = nros_tests::nros_cli_bin_path().expect("nros_cli_bin_path resolved");

    let out_dir = root.join("out");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("demo_bringup")
        .arg("demo_bringup") // dir input → discovers config/system_model.yaml
        .arg("--workspace")
        .arg(root)
        .arg("--out-dir")
        .arg(&out_dir)
        .current_dir(root)
        .output()
        .expect("spawn nros plan");

    // `nros plan` prints this line ONLY after it discovered + loaded the
    // committed model under the non-member `demo_bringup` dir. Its presence is
    // the dirwalk-discovery proof; the plan may then fail on missing source
    // metadata, which is out of scope here.
    let stderr = String::from_utf8_lossy(&result.stderr);
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stderr.contains("SystemModel demo_bringup/config/system_model.yaml")
            || stderr.contains("demo_bringup/config/system_model.yaml"),
        "nros plan did not discover the sibling bringup's committed model \
         (exit={:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        result.status.code(),
    );
}

/// `nros plan` finds the bringup pkg by dirwalk even when the top-level
/// workspace `Cargo.toml` omits `exclude` (only `members` is declared). The
/// bringup dir has no Cargo.toml, so cargo metadata won't see it — dirwalk
/// is the only loader path.
#[test]
fn nros_plan_discovers_sibling_bringup_via_dirwalk() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    }
    let td = tempfile::tempdir().expect("tempdir");
    stage_fixture(
        td.path(),
        r#"[workspace]
resolver = "2"
members = ["talker_pkg"]

[workspace.metadata.nros]
default_system = "demo_bringup"
"#,
    );
    run_plan_and_assert(td.path());
}

/// Same fixture with the canonical Path A shape: bringup dir listed in
/// `[workspace] exclude`. Documents the recommended layout — dirwalk still
/// finds it, exclude just keeps `cargo build` quiet about the non-Cargo dir.
#[test]
fn nros_plan_finds_bringup_when_in_workspace_exclude() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    }
    let td = tempfile::tempdir().expect("tempdir");
    stage_fixture(
        td.path(),
        r#"[workspace]
resolver = "2"
members = ["talker_pkg"]
exclude = ["demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
"#,
    );
    run_plan_and_assert(td.path());
}
