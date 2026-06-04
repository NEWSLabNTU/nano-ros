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

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

/// Probe `play_launch_parser --version` to decide whether `nros plan` can
/// resolve a `system.launch.xml`. Used by Phase 214.N.3 skip-gates.
fn play_launch_parser_available() -> bool {
    Command::new("play_launch_parser")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

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
///     launch/system.launch.xml   # empty <launch/>
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

    fs::create_dir_all(root.join("demo_bringup/launch")).expect("mkdir demo_bringup/launch");
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
    fs::write(
        root.join("demo_bringup/launch/system.launch.xml"),
        "<launch>\n</launch>\n",
    )
    .expect("write demo_bringup/launch/system.launch.xml");
}

/// Invoke `nros plan demo_bringup demo_bringup/launch/system.launch.xml
/// --workspace <root> --out-dir <out>` and assert the resulting nros-plan.json
/// references the dirwalk-discovered bringup pkg.
fn run_plan_and_assert(root: &Path) {
    let nros = nros_tests::nros_cli_bin_path().expect("nros_cli_bin_path resolved");

    let out_dir = root.join("out");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("demo_bringup")
        .arg("demo_bringup/launch/system.launch.xml")
        .arg("--workspace")
        .arg(root)
        .arg("--out-dir")
        .arg(&out_dir)
        .current_dir(root)
        .output()
        .expect("spawn nros plan");
    assert!(
        result.status.success(),
        "nros plan failed (exit={:?})\nstdout:\n{}\nstderr:\n{}",
        result.status.code(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );

    let plan_path = out_dir.join("nros-plan.json");
    assert!(
        plan_path.is_file(),
        "nros-plan.json not written at {}",
        plan_path.display(),
    );
    let plan_text = fs::read_to_string(&plan_path).expect("read nros-plan.json");
    let plan: serde_json::Value =
        serde_json::from_str(&plan_text).expect("parse nros-plan.json as JSON");

    // The bringup pkg name lands in `plan.system` — that field is populated
    // ONLY when the planner successfully loaded the bringup dir's
    // system.toml, which on this fixture requires dirwalk discovery
    // (demo_bringup is NOT a workspace member, has no Cargo.toml, and is
    // never reachable from `cargo metadata`).
    assert_eq!(
        plan.get("system").and_then(|v| v.as_str()),
        Some("demo_bringup"),
        "plan.system != \"demo_bringup\" (dirwalk discovery missed the bringup pkg)\nplan:\n{plan_text}",
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
    // Phase 214.N.3 — `nros plan` shells out to `play_launch_parser` to
    // resolve `system.launch.xml`. When that parser is missing the verb
    // returns a hard error; gate the dirwalk-discovery assertion on its
    // availability rather than letting `nros plan` fail for an unrelated
    // tooling-precondition reason.
    if !play_launch_parser_available() {
        nros_tests::skip!(
            "play_launch_parser not on PATH (pip install play-launch-parser, or build its binary)"
        );
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
    // Phase 214.N.3 — same precondition as the sibling test above.
    if !play_launch_parser_available() {
        nros_tests::skip!(
            "play_launch_parser not on PATH (pip install play-launch-parser, or build its binary)"
        );
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
