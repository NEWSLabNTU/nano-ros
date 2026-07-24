//! `nros plan` - generate host-side orchestration plan.
//!
//! Phase 212.L.6: the positional `<launch_file>` now also accepts a
//! **package directory** (Cargo / CMake pkg, or a bringup pkg). When a
//! directory is passed we route through
//! [`orchestration::launch_synth::resolve_launch`] which either picks a
//! convention-named launch file under `<dir>/launch/` or synthesises a
//! one-node `<launch>` body in-memory for self-bringup pkgs.

use crate::orchestration::{
    launch_synth::{is_self_entry_pkg, resolve_launch},
    planner::{PlanOptions, plan_system},
};
use clap::Args as ClapArgs;
use eyre::Result;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// System package name used for build/<system_pkg>/nros output.
    /// When omitted, derived from the `<launch_file>` directory's
    /// pkg name (Phase 212.L.7 self-bringup shape).
    pub system_pkg: String,

    /// ROS 2 launch file to parse, **or** a package directory to resolve
    /// via the Phase 212.L.6 multi-launch policy (pkg-named →
    /// system.launch.xml → single-file → synth for self-bringup pkgs).
    /// When omitted in a Phase 212.L.7 single-arg invocation
    /// (`nros plan <pkg-dir>`), defaults to the `<system_pkg>` argument
    /// (treated as a directory path then).
    #[arg(default_value = "")]
    pub launch_file: PathBuf,

    /// Precomputed play_launch record.json to use instead of parsing launch_file
    #[arg(long)]
    pub record: Option<PathBuf>,

    /// Plan from a resolved SystemModel (canonical). With a package-dir
    /// input, a committed `<dir>/config/system_model.yaml` is discovered
    /// automatically; this flag overrides with an explicit path.
    #[arg(long, value_name = "system_model.yaml")]
    pub model: Option<PathBuf>,

    /// Phase 212.L.6 — when `<launch_file>` is a directory, prefer
    /// `<dir>/launch/<file>` (or cwd-relative / absolute as fallback).
    #[arg(long = "file")]
    pub file: Option<String>,

    /// Phase 212.L.6 — disambiguates the synthesised `<node exec="…">`
    /// when the package declares multiple `[[bin]]` / `add_executable`
    /// targets.
    #[arg(long = "exec")]
    pub exec: Option<String>,

    /// Workspace root containing colcon-like src/* packages
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Output root for orchestration artifacts
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Existing source metadata JSON artifact
    #[arg(long = "metadata")]
    pub metadata: Vec<PathBuf>,

    /// ROS launch manifest YAML artifact
    #[arg(long = "manifest")]
    pub manifests: Vec<PathBuf>,

    /// Phase 255 Wave 4 — RMW override, the TOP of the precedence ladder
    /// (`--rmw` > `[deploy.<t>].rmw` > `[system].rmw` > `zenoh`). Sets
    /// `plan.build.rmw` regardless of `system.toml` / the `[build].rmw` overlay.
    #[arg(long = "rmw")]
    pub rmw: Option<String>,

    /// Phase 256 — select the `[deploy.<t>]` the planner resolves per-target
    /// values against (RMW override, build tuning, domain/locator). Omitted →
    /// `[system].default_target` → the sole `[deploy.<t>]` → target-agnostic.
    #[arg(long = "target")]
    pub target: Option<String>,

    /// Launch arguments forwarded as name:=value or name=value
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub launch_args: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    let workspace_root = args.workspace.unwrap_or(std::env::current_dir()?);

    // Phase 212.L.7 — single-arg self-bringup shape. When the user
    // passes only `<pkg-dir>` (no positional launch_file), `clap`
    // sees `launch_file` as the empty path; default it to the
    // `system_pkg` argument treated as a directory path. The
    // launch_synth resolver handles the rest.
    let launch_input_path = if args.launch_file.as_os_str().is_empty() {
        PathBuf::from(&args.system_pkg)
    } else {
        args.launch_file.clone()
    };

    // Derive the system_pkg from the dir name when the user passed a
    // dir as `<system_pkg>` (single-arg self-bringup shape).
    let system_pkg =
        if args.launch_file.as_os_str().is_empty() && PathBuf::from(&args.system_pkg).is_dir() {
            PathBuf::from(&args.system_pkg)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&args.system_pkg)
                .to_string()
        } else {
            args.system_pkg.clone()
        };

    let out_root = args
        .out_dir
        .unwrap_or_else(|| workspace_root.join("build").join(&system_pkg).join("nros"));

    // Phase 212.L.6: the positional `launch_file` may be either an
    // existing file (legacy path) or a package directory. Resolve to a
    // real on-disk path that the external `play_launch_parser` binary
    // can consume — synthesised XML is written to a temp file whose
    // lifetime is tied to `_materialised` and removed when planning
    // returns.
    //
    // Phase 212.L.7: when the pkg dir carries BOTH
    // `[package.metadata.nros.node]` AND `[package.metadata.nros.entry]`
    // it eats its own Entry role (single-pkg dev loop). The L.6 resolver
    // covers both branches uniformly — real launch file under
    // `<dir>/launch/` first, synthesised `<launch><node …/>` body when
    // absent. We just log when L.7 self-entry mode kicks in so users
    // see what shape the CLI picked.
    // R-code plan rework — canonical input: a resolved SystemModel.
    // Explicit `--model`, else convention discovery on a package-dir input
    // (`<dir>/config/system_model.yaml`). Model mode synthesizes the record
    // the planner consumes (no launch XML parse); the model path rides
    // `launch_file` provenance in nros-plan.json.
    let discovered_model: Option<PathBuf> = args.model.clone().or_else(|| {
        let conv = launch_input_path.join("config/system_model.yaml");
        (launch_input_path.is_dir() && conv.exists()).then_some(conv)
    });
    if let Some(model_path) = &discovered_model {
        eprintln!(
            "nros plan: planning from SystemModel {} (pass --model to override)",
            model_path.display()
        );
        let model = crate::orchestration::model_ingest::load_model(model_path)?;
        let record = crate::orchestration::model_ingest::plan_record_from_model(&model);
        let tmp = tempfile::NamedTempFile::new()?;
        std::fs::write(tmp.path(), serde_json::to_string_pretty(&record)?)?;
        let output = plan_system(PlanOptions {
            system_pkg,
            workspace_root,
            launch_file: model_path.clone(),
            record_file: Some(tmp.path().to_path_buf()),
            out_root,
            metadata_files: args.metadata,
            manifest_files: args.manifests,
            launch_args: args.launch_args,
            rmw: args.rmw,
            target: args.target,
        })?;
        drop(tmp);
        eprintln!(
            "nros plan: wrote {} and {}",
            output.record_path.display(),
            output.plan_path.display()
        );
        return Ok(());
    }
    // phase-296 R3 — the launch-XML resolution path is transitional; a
    // bringup with a committed model never reaches it (discovery above).
    crate::deprecation::warn_legacy_bake("nros plan (launch-XML resolution)");

    let (resolved_path, _materialised) = if launch_input_path.is_dir() {
        if is_self_entry_pkg(&launch_input_path) {
            eprintln!(
                "nros plan: {} is a self-entry pkg \
                 ([package.metadata.nros.node] + [package.metadata.nros.entry]); \
                 resolving launch via L.6 (real launch.xml or synth)",
                launch_input_path.display()
            );
        }
        let input = resolve_launch(
            &launch_input_path,
            args.file.as_deref(),
            args.exec.as_deref(),
        )?;
        let materialised = input.materialise()?;
        (materialised.path.clone(), Some(materialised))
    } else {
        (launch_input_path.clone(), None)
    };

    let output = plan_system(PlanOptions {
        system_pkg,
        workspace_root,
        launch_file: resolved_path,
        record_file: args.record,
        out_root,
        metadata_files: args.metadata,
        manifest_files: args.manifests,
        launch_args: args.launch_args,
        rmw: args.rmw,
        target: args.target,
    })?;

    // `_materialised` keeps the synthesised temp file alive through
    // `plan_system`; drop it now (RAII removes the temp file).
    drop(_materialised);

    eprintln!(
        "nros plan: wrote {} and {}",
        output.record_path.display(),
        output.plan_path.display()
    );
    Ok(())
}
