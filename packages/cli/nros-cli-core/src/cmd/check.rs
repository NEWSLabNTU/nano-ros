//! `nros check` - validate a generated nros-plan.json, a root nros.toml, or
//! (Phase 212.F) a `<bringup>` pkg directory for pure-declarative shape.

use crate::{
    cmd::{
        bringup::lint_bringup,
        check_workspace::check_workspace,
        emit_package_xml::{DriftStatus, check_drift},
    },
    orchestration::{planner::check_plan_file, root_config::WorkspaceConfig},
};
use clap::Args as ClapArgs;
use eyre::Result;
use std::path::PathBuf;

#[derive(Debug, Default, ClapArgs)]
pub struct Args {
    /// Path to nros-plan.json, a root nros.toml (Phase 172 WP-A), or a
    /// `<bringup>` pkg directory when `--bringup` is set (Phase 212.F).
    #[arg(default_value = "build/nros/nros-plan.json")]
    pub plan: PathBuf,

    /// Phase 212.G.2 — also check a package directory for generated
    /// `package.xml` drift (a generator-marked file edited by hand).
    /// May be passed multiple times.
    #[arg(long = "package-xml-drift")]
    pub package_xml_drift: Vec<PathBuf>,

    /// Phase 212.F — lint the `plan` argument as a `<bringup>` package
    /// directory: reject `Cargo.toml`, `CMakeLists.txt`, `src/`, or any
    /// nested `add_executable(`. The bringup package must be pure
    /// declarative (see docs/design/0024-multi-node-workspace-layout.md §4).
    #[arg(long)]
    pub bringup: bool,

    /// Phase 212.L — walk a workspace root and run L.4 / L.8 / L.11:
    /// `<pkg>::<Class>` enforcement on every `[[component]]` row, stray
    /// `system.toml` next to a component pkg, and per-pkg
    /// `.cargo/config.toml` carrying `[patch.crates-io]` (warn-only). When
    /// the flag is passed with no value the workspace defaults to the
    /// current directory.
    #[arg(long, num_args = 0..=1, default_missing_value = ".", value_name = "DIR")]
    pub workspace: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    // Phase 212.L — `--workspace [<dir>]` runs the workspace-walk lint.
    if let Some(ws_root) = args.workspace.as_deref() {
        let report = check_workspace(ws_root)?;
        for w in &report.warnings {
            eprintln!("nros check: warning: {w}");
        }
        eprintln!(
            "nros check: ok (workspace {}, {} pkg(s), {} warning(s))",
            ws_root.display(),
            report.pkgs_visited,
            report.warnings.len()
        );
        return Ok(());
    }

    // Phase 212.F — `--bringup` switches the `plan` argument into a directory
    // path and runs the pure-declarative lint.
    if args.bringup {
        lint_bringup(&args.plan)?;
        eprintln!(
            "nros check: ok (bringup pkg {} is pure declarative)",
            args.plan.display()
        );
        return Ok(());
    }

    // Phase 212.F.2 — cwd-bringup auto-detection. When the user runs a bare
    // `nros check` from inside a bringup pkg (default `plan` arg, no
    // `--bringup` flag) AND the cwd carries `package.xml + system.toml`,
    // auto-route into the bringup lint so the manual smoke-test from the
    // F.2 task brief — `cd demo_bringup && nros check` — exits 0 / 1
    // without the user spelling out `--bringup .`.
    let plan_arg_is_default = args.plan == PathBuf::from("build/nros/nros-plan.json");
    if plan_arg_is_default && !args.plan.exists() {
        if let Ok(cwd) = std::env::current_dir() {
            if cwd.join("package.xml").is_file() && cwd.join("system.toml").is_file() {
                lint_bringup(&cwd)?;
                eprintln!(
                    "nros check: ok (bringup pkg {} is pure declarative)",
                    cwd.display()
                );
                return Ok(());
            }
        }
    }

    // Phase 212.G.2 — drift sweep over any explicitly named pkg dirs runs
    // first so warnings surface even when the plan check exits early.
    for pkg_dir in &args.package_xml_drift {
        match check_drift(pkg_dir)? {
            DriftStatus::Drift { on_disk_path } => {
                eprintln!(
                    "nros check: warning: {} carries the generated marker but \
                     differs from a fresh `nros emit package-xml` — \
                     re-run the emit to discard local edits",
                    on_disk_path.display()
                );
            }
            DriftStatus::Absent | DriftStatus::Clean | DriftStatus::HandWritten => {}
        }
    }

    // A `.toml` argument is the workspace-root deployment config; anything
    // else is a generated plan. `WorkspaceConfig::load` validates as it parses.
    if args.plan.extension().is_some_and(|e| e == "toml") {
        let cfg = WorkspaceConfig::load(&args.plan)?;
        let systems = cfg.systems.len() + usize::from(cfg.system.is_some());
        eprintln!(
            "nros check: ok ({} system(s), {} deploy target(s), {})",
            systems,
            cfg.deploy.len(),
            args.plan.display()
        );
        return Ok(());
    }

    let report = check_plan_file(&args.plan)?;
    if report.errors == 0 {
        for message in &report.messages {
            eprintln!("nros check: warning: {message}");
        }
        eprintln!(
            "nros check: ok ({} warning(s), {})",
            report.warnings,
            args.plan.display()
        );
    }
    Ok(())
}

// Phase 172 — `[[bridge]]` per-node routing is now emitted by the generator
// (`register_bridges`: a bridge node per endpoint session + the generic-sub →
// generic-pub relay with `bridge_origin` echo suppression), so the former
// "routing not yet emitted" warning is gone. `[[domain]]` routing landed in
// Phase 172.K.5.
