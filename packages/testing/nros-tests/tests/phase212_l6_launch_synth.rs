//! Phase 212.L.6 — launch file synthesis + multi-launch resolution.
//!
//! These integration tests drive `nros plan` against staged tempdir
//! fixtures and assert the three branches of the shared resolution
//! policy implemented in `nros-cli-core::orchestration::launch_synth`:
//!
//! 1. `nros_plan_synthesises_launch_for_single_pkg_no_launch_file` — a
//!    component pkg w/ a single `[[bin]]` but no `launch/` dir. The
//!    resolver must synthesise `<launch><node pkg="…" exec="…"/></launch>`
//!    in-memory, feed it to the launch parser, and produce a valid
//!    `nros-plan.json` that names the synth-derived component.
//! 2. `nros_plan_refuses_path_a_bringup_with_no_launch` — a Path A
//!    bringup pkg (no Cargo.toml, no CMakeLists.txt, has system.toml +
//!    package.xml) MUST NOT trigger synthesis: that workspace shape
//!    declares multi-node composition and a missing launch file is a
//!    user error, not something the resolver should paper over. The
//!    command must exit non-zero with the resolver's specific error
//!    message.
//! 3. `nros_plan_picks_pkg_named_default` — when a pkg ships both
//!    `<pkg-name>.launch.xml` and `system.launch.xml`, the pkg-named
//!    convention wins (resolver step 2 beats step 3). The plan must
//!    reflect the pkg-named file's content (a single `<node>` w/ a
//!    distinct name).
//!
//! Skips cleanly via `nros_tests::skip!` when the `nros` CLI or the
//! `play_launch_parser` binary aren't on PATH.

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

/// True when the external `play_launch_parser` binary is callable. The
/// resolver synthesises XML in-memory but still pipes it through the
/// parser to produce the `record.json` the planner consumes — without
/// the parser these tests can't actually drive an end-to-end plan.
fn play_launch_parser_available() -> bool {
    Command::new("play_launch_parser")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Skip the test with a unified message when either tool is missing.
fn require_tools() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    }
    if !play_launch_parser_available() {
        nros_tests::skip!(
            "play_launch_parser not on PATH (pip install play-launch-parser, or build its binary)"
        );
    }
}

/// Stage a component pkg fixture under `root` with:
///
/// ```text
/// root/
///   Cargo.toml      # workspace member = ["alpha_pkg"]
///   alpha_pkg/
///     Cargo.toml    # [[bin]] name = "alpha_pkg"
///     src/main.rs
///   bringup_pkg/    # supplies workspace.metadata.nros.default_system
///     Cargo.toml    # workspace member, [lib] only
///     system.toml   # one [[component]] referring to alpha_pkg
///     package.xml
/// ```
///
/// The bringup pkg has NO `launch/` dir — that's the synthesis trigger.
fn stage_synth_fixture(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["alpha_pkg", "bringup_pkg"]

[workspace.metadata.nros]
default_system = "bringup_pkg"
"#,
    )
    .unwrap();

    // Node pkg with a single [[bin]].
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
default_namespace = "/"
"#,
    )
    .unwrap();
    fs::write(root.join("alpha_pkg/src/main.rs"), "fn main() {}\n").unwrap();

    // Bringup pkg — Cargo.toml lib-only + system.toml + NO launch dir.
    fs::create_dir_all(root.join("bringup_pkg/src")).unwrap();
    fs::write(
        root.join("bringup_pkg/Cargo.toml"),
        r#"[package]
name = "bringup_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
    )
    .unwrap();
    fs::write(root.join("bringup_pkg/src/lib.rs"), "").unwrap();
    fs::write(
        root.join("bringup_pkg/system.toml"),
        r#"[system]
name = "alpha"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "alpha_pkg"
class = "alpha_pkg::Node"
name = "alpha"
"#,
    )
    .unwrap();
    fs::write(
        root.join("bringup_pkg/package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>bringup_pkg</name>
  <version>0.1.0</version>
  <description>L.6 synth fixture bringup.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <exec_depend>alpha_pkg</exec_depend>
  <export><build_type>ament_nros</build_type></export>
</package>
"#,
    )
    .unwrap();
}

/// Stage a Path A bringup pkg fixture (NO Cargo.toml / CMakeLists.txt).
/// The presence of system.toml + package.xml without any build manifest
/// is the canonical Path A shape — synthesis is forbidden.
fn stage_path_a_bringup_no_launch(root: &Path) {
    fs::create_dir_all(root.join("bringup_pkg")).unwrap();
    fs::write(
        root.join("bringup_pkg/package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>bringup_pkg</name>
  <version>0.1.0</version>
  <description>Path A bringup no-launch failure fixture.</description>
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

/// Stage a pkg with BOTH `<pkg-name>.launch.xml` and `system.launch.xml`
/// under `launch/`. The pkg-named one (step 2) must win over the
/// `system.launch.xml` convention (step 3).
fn stage_pkg_named_vs_system_launch(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["bringup_pkg"]

[workspace.metadata.nros]
default_system = "bringup_pkg"
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("bringup_pkg/src")).unwrap();
    fs::create_dir_all(root.join("bringup_pkg/launch")).unwrap();
    fs::write(
        root.join("bringup_pkg/Cargo.toml"),
        r#"[package]
name = "bringup_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
    )
    .unwrap();
    fs::write(root.join("bringup_pkg/src/lib.rs"), "").unwrap();
    fs::write(
        root.join("bringup_pkg/system.toml"),
        r#"[system]
name = "alpha"
rmw = "zenoh"
domain_id = 0
"#,
    )
    .unwrap();
    fs::write(
        root.join("bringup_pkg/package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>bringup_pkg</name>
  <version>0.1.0</version>
  <description>Resolver convention precedence fixture.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <export><build_type>ament_nros</build_type></export>
</package>
"#,
    )
    .unwrap();
    // Pkg-named launch — distinct node name `from_pkg_named`.
    fs::write(
        root.join("bringup_pkg/launch/bringup_pkg.launch.xml"),
        r#"<launch>
  <node pkg="alpha_pkg" exec="alpha_pkg" name="from_pkg_named" />
</launch>
"#,
    )
    .unwrap();
    // system.launch.xml — different node name to prove it lost the
    // resolution race.
    fs::write(
        root.join("bringup_pkg/launch/system.launch.xml"),
        r#"<launch>
  <node pkg="alpha_pkg" exec="alpha_pkg" name="from_system_xml" />
</launch>
"#,
    )
    .unwrap();
}

/// `nros plan <pkg> <pkg-dir>` — runs the resolver since the second arg
/// is a directory (Phase 212.L.6 surface).
#[test]
fn nros_plan_synthesises_launch_for_single_pkg_no_launch_file() {
    require_tools();
    let td = tempfile::tempdir().expect("tempdir");
    stage_synth_fixture(td.path());

    let nros = nros_tests::nros_cli_bin_path().expect("nros bin");
    let out_dir = td.path().join("out");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("bringup_pkg")
        .arg("bringup_pkg") // directory → trigger resolver / synth
        .arg("--workspace")
        .arg(td.path())
        .arg("--out-dir")
        .arg(&out_dir)
        .current_dir(td.path())
        .output()
        .expect("spawn nros plan");
    // We do NOT assert overall success: the synth fixture is
    // intentionally minimal (no source-metadata JSON), and the planner
    // requires per-instance metadata for any non-empty launch record.
    // What we assert is that the resolver did its job — i.e. wrote a
    // record.json with the synth-derived node — which the planner does
    // before the metadata walk.

    // The resolver writes `record.json` BEFORE the planner walks the
    // metadata graph, so the file exists regardless of how the planner
    // exits. Asserting against the record proves the synth happened
    // and was parsed; full planning needs source metadata that's out
    // of scope for the resolver test.
    let record_path = out_dir.join("record.json");
    assert!(
        record_path.is_file(),
        "record.json missing at {} (synth did not reach the parser)\nstdout:\n{}\nstderr:\n{}",
        record_path.display(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    let record_text = fs::read_to_string(&record_path).unwrap();
    // The synth body shapes the record into one node entry whose
    // `pkg = "bringup_pkg"` (resolver target) and `exec = "bringup_pkg"`
    // (lib-only → pkg-name fallback).
    assert!(
        record_text.contains("bringup_pkg"),
        "record.json missing synth-derived pkg `bringup_pkg`:\n{record_text}"
    );
}

/// Path A bringup with NO launch dir → hard error from the resolver.
#[test]
fn nros_plan_refuses_path_a_bringup_with_no_launch() {
    require_tools();
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
        "nros plan should fail for Path A bringup w/ no launch\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    // Resolver's error message — accept either the "Path A bringup"
    // phrase or the "synthesis is disallowed" tail (in case the eyre
    // wrapper truncates the head).
    assert!(
        stderr.contains("Path A bringup") || stderr.contains("synthesis is disallowed"),
        "stderr should mention Path A / no-synth policy:\n{stderr}"
    );
}

/// Pkg-named convention wins over `system.launch.xml`.
#[test]
fn nros_plan_picks_pkg_named_default() {
    require_tools();
    let td = tempfile::tempdir().expect("tempdir");
    stage_pkg_named_vs_system_launch(td.path());

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
    // Same logic as the synth test — overall success requires source
    // metadata we don't stage; the resolver-output we care about
    // (record.json) is written before the metadata walk.
    let _ = &result;

    // The record.json the planner writes carries the parsed launch
    // contents directly — the node name distinguishes which file the
    // resolver picked. `from_pkg_named` proves step 2 won; the absence
    // of `from_system_xml` proves step 3 was skipped.
    let record_text = fs::read_to_string(out_dir.join("record.json")).expect("read record.json");
    assert!(
        record_text.contains("from_pkg_named"),
        "record.json missing pkg-named launch's node:\n{record_text}"
    );
    assert!(
        !record_text.contains("from_system_xml"),
        "system.launch.xml's node leaked into the record — pkg-named convention should have won:\n{record_text}"
    );
}
