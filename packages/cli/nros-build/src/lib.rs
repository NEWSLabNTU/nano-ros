//! `nros-build` — Phase 212.N.4 codegen library for Entry-pkg `build.rs`.
//!
//! The Entry pkg (a user's tiny `main.rs` that calls `<Board as
//! BoardEntry>::run(setup)`) needs the body of `run_plan(runtime)`
//! emitted from a launch file + the workspace's component-pkg metadata.
//! Per phase 212.N.4 spec, this library is the seam:
//!
//! ```ignore
//! // build.rs
//! fn main() -> anyhow::Result<()> {
//!     nros_build::generate_run_plan("launch/system.launch.xml")?;
//!     Ok(())
//! }
//! ```
//!
//! ```ignore
//! // main.rs
//! include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));
//!
//! fn main() {
//!     <MyBoard as BoardEntry>::run(|runtime| run_plan(runtime)).unwrap();
//! }
//! ```
//!
//! The emitted `run_plan(runtime)` fn is **board-agnostic** — board
//! choice lives in user `main.rs`'s `Board::run` call. The fn body
//! walks the planner's component-instance list and emits one
//! `<component_pkg>::register(runtime)?;` call per instance.
//!
//! ## Reuse
//!
//! Internally calls
//! [`nros_cli_core::orchestration::planner::plan_system`] to produce
//! an `NrosPlan` (the same shape `nros codegen-system` emits) and then
//! walks the plan's `instances` to emit the register calls. The plan
//! is also written to `$OUT_DIR/nros-system/nros-plan.json` for
//! downstream tooling that wants to introspect it.
//!
//! ## Status
//!
//! Phase 212.N.4 v0.1 — single public entry-point. Launch-arg parsing,
//! per-component QoS overlays, lifecycle hooks, and shared-state
//! regions are NOT yet propagated into the emitted body; they live in
//! the planner output and will be wired in over follow-up patches as
//! the Entry pkg shape stabilises.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use eyre::{Context, Result};

pub mod emit;
// Phase 219.A — `pkg_index` and `launch_parser` moved into
// `nros-cli-core` (their canonical home — the new
// `nros_cli_core::codegen::entry` module consumes them directly and the
// cycle `nros-build → nros-cli-core` already exists). Re-export under
// the original module paths so every existing caller (the upstream
// `nros-macros` git dep included) compiles unchanged.
pub use nros_cli_core::{launch_parser, pkg_index};

// Phase 212.N.7 step-3 — `RuntimeError` moved to `nros-platform` (no_std)
// so embedded Entry pkgs don't need `nros-build` as a runtime dep
// (build-dep only). The emitted `run_plan` body references
// `::nros_platform::RuntimeError`. The previous `nros-build`-side
// definition is retired; downstream callers expecting it should
// import from `nros-platform` instead.

/// Options the Entry pkg's `build.rs` can tune before calling
/// [`generate_run_plan_with`].
///
/// Defaults pulled from cargo's build-script environment match the
/// shape every Entry pkg lives in. Override only when the launch
/// file lives outside the package or when feeding a non-default
/// system pkg name.
#[derive(Debug, Clone)]
pub struct Options {
    /// Path to the launch XML (or `<launch>.launch.py` after a
    /// future `launch.py` adapter lands). Resolved relative to
    /// `CARGO_MANIFEST_DIR` if not absolute.
    pub launch_file: PathBuf,
    /// Workspace root. Defaults to `CARGO_MANIFEST_DIR/../..` — the
    /// two-up pattern every Entry pkg in
    /// `examples/<plat>/rust/<example>/` lives at. Override when an
    /// Entry pkg sits elsewhere.
    pub workspace_root: PathBuf,
    /// System pkg name (typically the Entry pkg name itself; the
    /// planner reads its `[package.metadata.nros.entry]` section).
    pub system_pkg: String,
    /// Output directory for the emitted `run_plan.rs` +
    /// `nros-plan.json`. Defaults to `$OUT_DIR`.
    pub out_dir: PathBuf,
}

impl Options {
    /// Build `Options` from a launch-file path + cargo's build-script
    /// environment (`CARGO_MANIFEST_DIR`, `OUT_DIR`, `CARGO_PKG_NAME`).
    /// Panics if any of those env vars is missing — they are
    /// guaranteed to be set inside a cargo `build.rs`, so a missing
    /// one means the caller is using the API outside its intended
    /// context (typically a misconfigured test harness).
    pub fn from_env(launch_file: impl Into<PathBuf>) -> Self {
        let manifest_dir = PathBuf::from(
            env::var_os("CARGO_MANIFEST_DIR")
                .expect("nros-build: CARGO_MANIFEST_DIR not set (call from a build.rs)"),
        );
        let out_dir = PathBuf::from(
            env::var_os("OUT_DIR").expect("nros-build: OUT_DIR not set (call from a build.rs)"),
        );
        let system_pkg = env::var("CARGO_PKG_NAME")
            .expect("nros-build: CARGO_PKG_NAME not set (call from a build.rs)");
        let launch_file = launch_file.into();
        let launch_file = if launch_file.is_absolute() {
            launch_file
        } else {
            manifest_dir.join(&launch_file)
        };
        // Default workspace root = two parents up from manifest. Override via
        // the public field if your tree differs.
        let workspace_root = manifest_dir
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| manifest_dir.clone());
        Self {
            launch_file,
            workspace_root,
            system_pkg,
            out_dir,
        }
    }
}

/// Convenience entry point for the typical Entry-pkg `build.rs`:
///
/// ```ignore
/// fn main() -> eyre::Result<()> {
///     nros_build::generate_run_plan("launch/system.launch.xml")?;
///     Ok(())
/// }
/// ```
///
/// Resolves [`Options`] from the build-script environment, runs the
/// planner, and emits `$OUT_DIR/run_plan.rs`. Returns the emitted
/// file path so the caller can `println!("cargo:rerun-if-changed={}",
/// path.display())` if they want.
pub fn generate_run_plan(launch_file: impl Into<PathBuf>) -> Result<PathBuf> {
    generate_run_plan_with(&Options::from_env(launch_file))
}

/// Explicit-options variant of [`generate_run_plan`]. Use when the
/// Entry pkg lives outside the standard `examples/<plat>/rust/<name>/`
/// layout, or when you want to feed a hand-built `Options` from a
/// test harness.
pub fn generate_run_plan_with(opts: &Options) -> Result<PathBuf> {
    use nros_cli_core::orchestration::planner::{PlanOptions, plan_system};

    let plan_out_root = opts.out_dir.join("nros-system");
    fs::create_dir_all(&plan_out_root).with_context(|| {
        format!(
            "nros-build: create plan out dir {}",
            plan_out_root.display()
        )
    })?;

    // Drive the planner via the same shape `nros codegen-system` does.
    let plan_options = PlanOptions {
        system_pkg: opts.system_pkg.clone(),
        workspace_root: opts.workspace_root.clone(),
        launch_file: opts.launch_file.clone(),
        record_file: None,
        out_root: plan_out_root.clone(),
        metadata_files: Vec::new(),
        manifest_files: Vec::new(),
        nros_toml_files: Vec::new(),
        launch_args: Vec::new(),
        // Phase 255 — RMW resolved from the bringup's `system.toml`; build.rs has
        // no `--rmw` override.
        rmw: None,
    };
    let planning = plan_system(plan_options).context("nros-build: planner failed")?;
    println!(
        "cargo:rerun-if-changed={}",
        opts.launch_file.to_string_lossy()
    );
    println!("cargo:rerun-if-changed={}", planning.plan_path.display());

    let plan_json = fs::read_to_string(&planning.plan_path).with_context(|| {
        format!(
            "nros-build: read plan output {}",
            planning.plan_path.display()
        )
    })?;
    let plan: nros_cli_core::orchestration::plan::NrosPlan = serde_json::from_str(&plan_json)
        .with_context(|| {
            format!(
                "nros-build: deserialize plan output {}",
                planning.plan_path.display()
            )
        })?;

    let run_plan_path = opts.out_dir.join("run_plan.rs");
    // Phase 216 — emit two fns into the same `run_plan.rs`:
    //   1. `run_plan(runtime)`               — board-runtime register path
    //   2. `run_plan_register_dispatch(exec)` — framework Entry-pkg
    //      (RTIC/Embassy) dispatch-slot registry path
    // The second fn is additive — the existing `nros::main!()`
    // `node_pkgs = [...]` path is unchanged. Consumers opt in by
    // calling `run_plan_register_dispatch(&mut executor)` from
    // `#[init]` instead of having to declare each Node up front.
    let mut body = emit::emit_run_plan(&plan);
    body.push('\n');
    body.push_str(&emit::emit_run_plan_register_dispatch(&plan));
    fs::write(&run_plan_path, &body).with_context(|| {
        format!(
            "nros-build: write {} ({} bytes)",
            run_plan_path.display(),
            body.len()
        )
    })?;
    Ok(run_plan_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_from_env_resolves_relative_launch_file() {
        // Set the env the way cargo would.
        let tmp = tempfile::tempdir().expect("tempdir");
        let manifest_dir = tmp.path().join("pkg");
        fs::create_dir_all(&manifest_dir).unwrap();
        let out_dir = tmp.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();
        // SAFETY: test process owns these env vars; no other thread is
        // reading them concurrently here.
        unsafe {
            env::set_var("CARGO_MANIFEST_DIR", &manifest_dir);
            env::set_var("OUT_DIR", &out_dir);
            env::set_var("CARGO_PKG_NAME", "demo_entry");
        }

        let opts = Options::from_env("launch/system.launch.xml");
        assert_eq!(
            opts.launch_file,
            manifest_dir.join("launch/system.launch.xml")
        );
        assert_eq!(opts.out_dir, out_dir);
        assert_eq!(opts.system_pkg, "demo_entry");
    }
}
