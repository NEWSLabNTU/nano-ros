//! Phase 212.G — `nros emit package-xml` verb tests.
//!
//! Three coverage points:
//! 1. `emit_package_xml_from_cargo_ament_metadata` — Cargo component pkg
//!    with `[package.metadata.ament]` build/exec depends → `package.xml`
//!    carries those depends + name/version/maintainer + generated-by
//!    header.
//! 2. `emit_package_xml_from_system_toml_for_bringup` — bringup pkg dir
//!    with `system.toml` `[[component]]` blocks → `<exec_depend>` entries
//!    match every `pkg` value.
//! 3. `idempotent_round_trip` — running emit twice yields byte-identical
//!    `package.xml`.
//!
//! All three skip via `nros_tests::skip!` when the `nros` CLI isn't on
//! `$PATH` / `~/.nros/bin`.

use std::{fs, path::Path, process::Command};

use tempfile::TempDir;

fn nros_bin() -> std::path::PathBuf {
    nros_tests::nros_cli_bin_path().expect("nros CLI resolved (require_nros_cli gated above)")
}

fn run_emit_write(pkg_dir: &Path) -> std::process::Output {
    Command::new(nros_bin())
        .args(["emit", "package-xml", "--write"])
        .arg(pkg_dir)
        .output()
        .expect("spawn nros emit package-xml --write")
}

fn assert_success(out: &std::process::Output, ctx: &str) {
    assert!(
        out.status.success(),
        "{ctx} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn stage_cargo_component_pkg() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let toml = r#"[package]
name = "talker_pkg"
version = "0.4.2"
edition = "2021"
license = "Apache-2.0"
authors = ["Aeon <aeon@example.com>"]
description = "Phase 212.G talker component fixture"

[package.metadata.ament]
build_depend = ["rclrs", "std_msgs"]
exec_depend = ["std_msgs", "rclrs", "rcl_interfaces"]
"#;
    fs::write(dir.path().join("Cargo.toml"), toml).expect("write Cargo.toml");
    dir
}

fn stage_bringup_pkg() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let toml = r#"[system]
name = "demo_bringup"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::talker"
name = "talker"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::listener"
name = "listener"
"#;
    fs::write(dir.path().join("system.toml"), toml).expect("write system.toml");
    dir
}

#[test]
fn emit_package_xml_from_cargo_ament_metadata() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }

    let dir = stage_cargo_component_pkg();
    let out = run_emit_write(dir.path());
    assert_success(&out, "nros emit package-xml --write (component)");

    let xml_path = dir.path().join("package.xml");
    assert!(
        xml_path.is_file(),
        "expected {} to exist after --write",
        xml_path.display(),
    );
    let body = fs::read_to_string(&xml_path).expect("read package.xml");

    // Generated-by header (212.G.2). Match loosely on the word "generated"
    // inside an XML comment to avoid coupling to exact wording.
    assert!(
        body.contains("<!--") && body.to_lowercase().contains("generated"),
        "package.xml missing generated-by header:\n{body}",
    );

    // Standard fields.
    assert!(
        body.contains("<name>talker_pkg</name>"),
        "missing <name>:\n{body}",
    );
    assert!(
        body.contains("<version>0.4.2</version>"),
        "missing <version>:\n{body}",
    );
    assert!(body.contains("<maintainer"), "missing <maintainer>:\n{body}");

    // Depends from [package.metadata.ament] flow through.
    for dep in ["rclrs", "std_msgs"] {
        assert!(
            body.contains(&format!("<build_depend>{dep}</build_depend>")),
            "missing build_depend {dep}:\n{body}",
        );
    }
    for dep in ["rclrs", "std_msgs", "rcl_interfaces"] {
        assert!(
            body.contains(&format!("<exec_depend>{dep}</exec_depend>")),
            "missing exec_depend {dep}:\n{body}",
        );
    }
}

#[test]
fn emit_package_xml_from_system_toml_for_bringup() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }

    let dir = stage_bringup_pkg();
    let out = run_emit_write(dir.path());
    assert_success(&out, "nros emit package-xml --write (bringup)");

    let xml_path = dir.path().join("package.xml");
    assert!(
        xml_path.is_file(),
        "expected {} to exist after --write",
        xml_path.display(),
    );
    let body = fs::read_to_string(&xml_path).expect("read package.xml");

    // Generated-by header still present.
    assert!(
        body.contains("<!--") && body.to_lowercase().contains("generated"),
        "package.xml missing generated-by header:\n{body}",
    );

    // Bringup pkg name surfaces.
    assert!(
        body.contains("<name>demo_bringup</name>"),
        "missing bringup <name>:\n{body}",
    );

    // Every [[component]].pkg lands as an <exec_depend>.
    for dep in ["talker_pkg", "listener_pkg"] {
        assert!(
            body.contains(&format!("<exec_depend>{dep}</exec_depend>")),
            "missing exec_depend {dep}:\n{body}",
        );
    }
}

#[test]
fn idempotent_round_trip() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }

    let dir = stage_cargo_component_pkg();

    let first = run_emit_write(dir.path());
    assert_success(&first, "first emit");
    let xml_path = dir.path().join("package.xml");
    let body_a = fs::read_to_string(&xml_path).expect("read package.xml after first emit");

    let second = run_emit_write(dir.path());
    assert_success(&second, "second emit");
    let body_b = fs::read_to_string(&xml_path).expect("read package.xml after second emit");

    assert_eq!(
        body_a, body_b,
        "emit not idempotent:\n--- first ---\n{body_a}\n--- second ---\n{body_b}",
    );
}
