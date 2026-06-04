//! `nros build` — Phase 111.A.9 / Phase 172 flip.
//!
//! Two paths, auto-detected from the project root (cwd or `--project`):
//!
//! 1. **Deploy dispatch** (Phase 172 WP-A): when the root holds a loadable
//!    workspace `nros.toml`, `nros build <name>` aliases `nros deploy <name>`
//!    and bare `nros build` builds `[workspace].default`. The deploy runner
//!    generates + builds the entry lib and runs the target's command steps.
//!
//! 2. **Project-flavor delegation**: otherwise detect the flavor and delegate
//!    (Zephyr west, CMake, or Cargo) — precedence (highest first):
//!
//!      1. `prj.conf` present → Zephyr → `west build`
//!      2. `CMakeLists.txt` present + no `Cargo.toml` → `cmake -B build && cmake --build build`
//!      3. `Cargo.toml` present → `cargo build`
//!
//!    Mixed projects (Cargo.toml AND CMakeLists.txt) — common when a Rust
//!    crate produces a `staticlib` consumed by C/C++ — go through the
//!    cmake path. Heuristic: if `[lib].crate-type` in Cargo.toml contains
//!    `staticlib` AND CMakeLists.txt exists, prefer cmake.
//!
//! The legacy `--launch` one-shot and `--system-plan` pre-planned-build paths
//! were retired in the Phase 172 flip; building a system from a plan is now an
//! orchestration-library call (`orchestration::build::build_generated_package`)
//! driven by `nros deploy`.

use crate::{
    cmd::deploy,
    orchestration::root_config::{
        ManifestKind, WorkspaceConfig, probe_manifest_kind, resolve_workspace_root,
    },
};
use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, eyre};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Path to the project root (default: cwd)
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// nano-ros workspace root, forwarded to `nros deploy` for generated
    /// path-dependency / vendor-pin resolution. Also honored via the
    /// `NROS_WORKSPACE` environment variable inside the deploy runner.
    #[arg(long)]
    pub nano_ros_workspace: Option<PathBuf>,

    /// Deploy target from the root nros.toml (Phase 172 WP-A): `nros build
    /// <name>` is an alias for `nros deploy <name>`; bare `nros build` (in a
    /// workspace root) builds `[workspace].default`.
    pub deploy_name: Option<String>,

    /// Trailing arguments forwarded verbatim to the underlying tool
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub passthrough: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    super::emit_deprecation_warning(
        "build",
        "cargo build / cmake --build / west build / idf.py build",
    );

    let root = match args.project.clone() {
        Some(p) => p,
        None => std::env::current_dir()?,
    };

    // Phase 172 WP-A + W.1 slice 3 — root nros.toml deploy dispatch.
    //
    // `nros build <name>` aliases `nros deploy <name>`: walk up (Cargo-style)
    // to the enclosing `[workspace]` root, so it works from any member dir.
    // Bare `nros build` deploys `[workspace].default` ONLY when the cwd itself
    // is a workspace root — a member/standalone dir falls through to the
    // project-flavor autodetect below (build the local project, like
    // `cargo build`). A component / direct-mode `nros.toml` is not a workspace,
    // so it never hijacks the bare build.
    if let Some(name) = &args.deploy_name {
        let root_toml = resolve_workspace_root(&root)?.ok_or_else(|| {
            eyre!(
                "nros build {name}: no workspace nros.toml found from {} — deploy targets live \
                 in a [workspace] root",
                root.display()
            )
        })?;
        return deploy::run(deploy::Args {
            name: Some(name.clone()),
            config: root_toml,
            nano_ros_workspace: args.nano_ros_workspace.clone(),
            dry_run: false,
        });
    }
    let root_toml = root.join("nros.toml");
    if root_toml.is_file() && probe_manifest_kind(&root_toml)? == ManifestKind::Workspace {
        let cfg = WorkspaceConfig::load(&root_toml)?;
        let name = cfg.workspace.default.clone().ok_or_else(|| {
            eyre!(
                "nros build: ./nros.toml is a workspace but has no [workspace].default — \
                 pass `nros build <name>`"
            )
        })?;
        return deploy::run(deploy::Args {
            name: Some(name),
            config: root_toml,
            nano_ros_workspace: args.nano_ros_workspace.clone(),
            dry_run: false,
        });
    }

    // Phase 187.6 (Method A): lazy-install host tools a native build needs
    // (zenohd for the host router), then put them on PATH for the spawned build;
    // no-op away from a nano-ros workspace / with NROS_NO_AUTO_SETUP.
    let bins = crate::cmd::setup::ensure_tools("native", args.nano_ros_workspace.as_deref())?;
    crate::cmd::setup::activate_store_path(&bins);

    let flavor = detect_flavor(&root)?;
    eprintln!("nros build: flavor = {flavor:?} ({})", root.display());

    let mut cmd = match flavor {
        Flavor::West => {
            let mut c = Command::new("west");
            c.arg("build");
            c
        }
        Flavor::Cmake => {
            // `cmake -B build && cmake --build build` chained as one
            // shell, but we keep them as two child processes so we don't
            // need a shell.
            let configure = Command::new("cmake")
                .current_dir(&root)
                .args(["-B", "build", "-S", "."])
                .args(&args.passthrough)
                .status()
                .wrap_err("failed to invoke `cmake -B build`")?;
            if !configure.success() {
                return Err(eyre!(
                    "cmake configure failed (exit {})",
                    configure.code().unwrap_or(-1)
                ));
            }
            let mut c = Command::new("cmake");
            c.arg("--build").arg("build");
            c
        }
        Flavor::Cargo => {
            let mut c = Command::new("cargo");
            c.arg("build");
            c
        }
    };
    if !matches!(flavor, Flavor::Cmake) {
        cmd.args(&args.passthrough);
    }
    cmd.current_dir(&root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd
        .status()
        .wrap_err_with(|| format!("failed to invoke build for {flavor:?}"))?;
    if !status.success() {
        return Err(eyre!("build failed (exit {})", status.code().unwrap_or(-1)));
    }
    Ok(())
}

#[derive(Debug)]
enum Flavor {
    West,
    Cmake,
    Cargo,
}

fn detect_flavor(root: &Path) -> Result<Flavor> {
    let has_prj_conf = root.join("prj.conf").is_file();
    let has_cmake = root.join("CMakeLists.txt").is_file();
    let cargo_toml = root.join("Cargo.toml");
    let has_cargo = cargo_toml.is_file();

    if has_prj_conf {
        return Ok(Flavor::West);
    }

    if has_cmake && has_cargo && produces_staticlib(&cargo_toml).unwrap_or(false) {
        return Ok(Flavor::Cmake);
    }
    if has_cargo {
        return Ok(Flavor::Cargo);
    }
    if has_cmake {
        return Ok(Flavor::Cmake);
    }
    Err(eyre!(
        "no build flavor detected at {}: expected prj.conf (Zephyr), \
         CMakeLists.txt (CMake), or Cargo.toml (Rust)",
        root.display()
    ))
}

fn produces_staticlib(cargo_toml: &Path) -> Result<bool> {
    let raw = fs::read_to_string(cargo_toml)?;
    let doc: toml::Value = toml::from_str(&raw)?;
    let Some(lib) = doc.get("lib") else {
        return Ok(false);
    };
    let Some(crate_type) = lib.get("crate-type").or_else(|| lib.get("crate_type")) else {
        return Ok(false);
    };
    Ok(match crate_type {
        toml::Value::Array(arr) => arr.iter().any(|v| v.as_str() == Some("staticlib")),
        toml::Value::String(s) => s == "staticlib",
        _ => false,
    })
}
