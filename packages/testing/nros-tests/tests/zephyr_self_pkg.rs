//! Phase 212.M-F.3 — Zephyr H.1 adapter shim accepts self-pkg bringup.
//!
//! The M-F.2 nros-cli planner accepts an L.7 self-pkg bringup: a single
//! Node / Application Cargo pkg carrying
//! `[package.metadata.nros.deploy.zephyr]` (no sibling Path A
//! `system.toml`). This file drives the M-F.3 shim extension that
//! teaches `zephyr/cmake/nros_system_generate.cmake::
//! _nros_system_resolve_bringup()` to accept that shape — both via the
//! Cargo.toml metadata table AND via the L.9 cmake-side
//! `nano_ros_deploy(TARGET zephyr…)` pattern (C/C++ examples, M-Wave 2A
//! deferred).
//!
//! Tests:
//!   1. `zephyr_self_pkg_rust_builds_via_shim` — stage a minimal Rust
//!      self-pkg, run `west build`, assert the bake fires.
//!   2. `zephyr_self_pkg_resolve_bringup_handles_relative_path` — same
//!      fixture, invoked via both `.` and a sibling-dir name, to
//!      confirm both resolver candidate paths still work.
//!
//! Skip discipline: any missing prereq (`west`, `ZEPHYR_BASE`, `nros
//! codegen-system` verb) panics via `nros_tests::skip!` — silent
//! early-return is forbidden per CLAUDE.md.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn have(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ensure_zephyr_base() {
    if std::env::var("ZEPHYR_BASE").is_ok() {
        return;
    }
    if let Some(ws) = nros_tests::zephyr::zephyr_workspace_path() {
        let candidate = ws.join("zephyr");
        if candidate.join("zephyr-env.sh").exists() {
            // SAFETY: nextest runs each test in its own process; set-before-use
            // mirrors phase212_h1_zephyr.rs.
            unsafe { std::env::set_var("ZEPHYR_BASE", &candidate) };
        }
    }
}

fn require_prereqs() {
    ensure_zephyr_base();
    if !have("west") {
        nros_tests::skip!("west CLI not on PATH — install Zephyr SDK + west");
    }
    if std::env::var("ZEPHYR_BASE").is_err() {
        nros_tests::skip!(
            "ZEPHYR_BASE unset and no in-tree zephyr-workspace/zephyr — \
             run `just zephyr setup`"
        );
    }
    let nros_help = Command::new("nros")
        .args(["codegen-system", "--help"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status();
    match nros_help {
        Ok(s) if s.success() => {}
        _ => nros_tests::skip!(
            "`nros codegen-system` verb unavailable — Phase 212.E not landed in installed CLI"
        ),
    }
}

/// Stage a minimal Zephyr Rust self-pkg directly under `pkg_dir`. The
/// pkg carries `[package.metadata.nros.component]` +
/// `[package.metadata.nros.deploy.zephyr]` and ships a thin
/// `CMakeLists.txt` whose ONLY consumer-surface call is
/// `nros_system_generate(<bringup-arg>)`. `bringup_arg` lets each test
/// re-stage to invoke the shim via either `.` or a sibling-dir name.
fn stage_zephyr_self_pkg(pkg_dir: &Path, pkg_name: &str, bringup_arg: &str) {
    fs::create_dir_all(pkg_dir.join("src")).unwrap();
    fs::write(
        pkg_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[lib]
name = "rustapp"
crate-type = ["rlib", "staticlib"]

[package.metadata.nros.component]
class = "{pkg_name}::Node"
name = "selfpkg"
default_namespace = "/"

[package.metadata.nros.deploy.zephyr]
board = "native_sim/native/64"
rmw = "zenoh"
domain_id = 0

[workspace]
"#
        ),
    )
    .unwrap();
    fs::write(pkg_dir.join("src/lib.rs"), "#![no_std]\npub struct Node;\n").unwrap();
    fs::write(pkg_dir.join("prj.conf"), "CONFIG_NROS=y\n").unwrap();
    fs::write(
        pkg_dir.join("CMakeLists.txt"),
        format!(
            r#"cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{{ZEPHYR_BASE}})
project({pkg_name})
nros_system_generate({bringup_arg})
"#
        ),
    )
    .unwrap();
}

/// Run `west build` against `app_dir` and return (status, build_dir).
fn run_west_build(app_dir: &Path, build_dir: &Path) -> std::process::ExitStatus {
    let _ = fs::remove_dir_all(build_dir);
    Command::new("west")
        .args(["build", "-b", "native_sim/native/64", "-d"])
        .arg(build_dir)
        .arg(app_dir)
        .args(["--", "-DCONF_FILE=prj.conf"])
        .status()
        .expect("invoke west build")
}

/// 1. Self-pkg with `nros_system_generate(.)` builds via the shim. The
///    only assertion that scopes the shim contract is "configure ran +
///    `system_main.c` / `system_config.h` landed in the build tree" —
///    a full ELF link needs the rest of the runtime, which is out of
///    scope here.
#[test]
fn zephyr_self_pkg_rust_builds_via_shim() {
    require_prereqs();

    let td = tempfile::tempdir().expect("tempdir");
    let pkg = td.path().join("alpha_pkg");
    stage_zephyr_self_pkg(&pkg, "alpha_pkg", ".");

    let build_dir = workspace_root().join("build/phase212-mf3-zephyr-rust");
    let status = run_west_build(&pkg, &build_dir);

    // The shim FATAL_ERRORs early on resolver failure, so a non-zero
    // configure exit when the bake outputs are missing == shim regress.
    let baked = build_dir.join("nros-system");
    let config_h = baked.join("system_config.h");
    let main_c = baked.join("system_main.c");
    assert!(
        config_h.exists() && main_c.exists(),
        "M-F.3 shim self-pkg bake missing: {}\n  configure rc={:?}, \
         baked dir contents: {:?}",
        baked.display(),
        status.code(),
        fs::read_dir(&baked).map(|it| it.flatten().count()).ok(),
    );
}

/// 2. Both `nros_system_generate(.)` and the sibling-dir form resolve
///    to the self-pkg dir. A test fixture w/ two siblings — `caller/`
///    holds the CMakeLists, `alpha_pkg/` is the self-pkg — invoking
///    `nros_system_generate(alpha_pkg)` from `caller/` must still hit
///    the shim's M-F.3 self-pkg branch (no `system.toml` in
///    `alpha_pkg/`).
#[test]
fn zephyr_self_pkg_resolve_bringup_handles_relative_path() {
    require_prereqs();

    let td = tempfile::tempdir().expect("tempdir");

    // Sibling layout: workspace_root/{caller,alpha_pkg}.
    let caller = td.path().join("caller");
    let pkg = td.path().join("alpha_pkg");
    stage_zephyr_self_pkg(&pkg, "alpha_pkg", "");

    fs::create_dir_all(&caller).unwrap();
    fs::write(caller.join("prj.conf"), "CONFIG_NROS=y\n").unwrap();
    fs::write(
        caller.join("CMakeLists.txt"),
        r#"cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(caller)
nros_system_generate(alpha_pkg)
"#,
    )
    .unwrap();

    let build_dir = workspace_root().join("build/phase212-mf3-zephyr-sibling");
    let status = run_west_build(&caller, &build_dir);

    let baked = build_dir.join("nros-system");
    assert!(
        baked.join("system_config.h").exists() && baked.join("system_main.c").exists(),
        "sibling self-pkg resolve broke: {} (configure rc={:?})",
        baked.display(),
        status.code()
    );
}
