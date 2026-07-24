//! `nros plan` - generate host-side orchestration plan.
//!
//! R-code (phase-296 R4): the canonical input is a resolved SystemModel —
//! convention-discovered from a package-dir input
//! (`<dir>/config/system_model.yaml`), overridden with `--model`, or a
//! pre-baked record via `--record`. A launchless self-bringup pkg plans a
//! synthesized 1-node model. The launch-XML parse path is deleted.

use crate::orchestration::planner::{PlanOptions, plan_system};
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

    let discovered_model: Option<PathBuf> = args.model.clone().or_else(|| {
        // Dir input: `<dir>/config/system_model.yaml`. File input (the cmake
        // workspace seam passes `<bringup>/launch/<f>.launch.xml`): hop to
        // the bringup dir — the launch file's `launch/` parent's parent.
        let conv = if launch_input_path.is_dir() {
            launch_input_path.join("config/system_model.yaml")
        } else {
            launch_input_path
                .parent()
                .and_then(std::path::Path::parent)
                .map(|b| b.join("config/system_model.yaml"))?
        };
        conv.exists().then_some(conv)
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
    // R-code precondition #3 — the L.7 self-bringup dev loop (`nros plan
    // <pkg-dir>` with no launch files) synthesizes a 1-node MODEL instead of
    // 1-node launch XML: same discovered (pkg, exec) inputs, planned through
    // the same record plumbing model mode uses — no parser round trip.
    if args.record.is_none()
        && launch_input_path.is_dir()
        && args.file.is_none()
        && crate::orchestration::launch_synth::is_self_bringup_eligible(&launch_input_path)
        && crate::orchestration::launch_synth::enumerate_launch_files(&launch_input_path).is_empty()
    {
        let pkg_name = crate::orchestration::launch_synth::discover_pkg_name(&launch_input_path)?;
        let exec_name = match args.exec.as_deref() {
            Some(e) => e.to_string(),
            None => crate::orchestration::launch_synth::discover_exec_target(
                &launch_input_path,
                &pkg_name,
            )?,
        };
        eprintln!(
            "nros plan: {} is a self-bringup pkg with no launch files; \
             planning a synthesized 1-node model ({pkg_name}/{exec_name})",
            launch_input_path.display()
        );
        let model =
            crate::orchestration::launch_synth::synthesise_self_model(&pkg_name, &exec_name);
        let record = crate::orchestration::model_ingest::plan_record_from_model(&model);
        let tmp = tempfile::NamedTempFile::new()?;
        std::fs::write(tmp.path(), serde_json::to_string_pretty(&record)?)?;
        let output = plan_system(PlanOptions {
            system_pkg,
            workspace_root,
            launch_file: launch_input_path.clone(),
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
    // R-code.1 — the launch-XML resolution path is DELETED. A dir input
    // lands here only when it carries launch files but no committed model;
    // a file input lands here only without --record. Both must resolve.
    if args.record.is_none() {
        eyre::bail!(
            "`{}` has no committed SystemModel and the launch-XML parse path \
             was removed (phase-296 R4). Resolve one and commit it:\n  \
             play_launch resolve <bringup>/launch/<file>.launch.xml \
             [--system <bringup>/system.toml] -o \
             <bringup>/config/system_model.yaml\n(convention discovery plans \
             it; `--model <path>` overrides; a pre-baked record via --record \
             also works)",
            launch_input_path.display()
        );
    }
    let resolved_path = launch_input_path.clone();

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

    eprintln!(
        "nros plan: wrote {} and {}",
        output.record_path.display(),
        output.plan_path.display()
    );
    Ok(())
}
