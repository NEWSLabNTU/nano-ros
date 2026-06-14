//! `nros new <name>` — Phase 111.A.4.
//!
//! Forwards to `cargo_nano_ros::scaffold::scaffold_package` so the CLI
//! stays in lockstep with the shared scaffolding implementation.
//! Use-case (`talker` / `listener` / `service` / `action`) and RMW-choice
//! diversification are accepted at the CLI for forward-compat but
//! currently affect only the printed "Next steps" banner — full
//! per-use-case template trees land alongside the Phase 112 example
//! sweep.

use cargo_nano_ros::scaffold::{
    ComponentScaffoldConfig, ScaffoldConfig, scaffold_component, scaffold_package,
};
use clap::Args as ClapArgs;
use eyre::{Result, bail};
use std::path::PathBuf;

use crate::cmd::{
    new_system::{BringupScaffold, scaffold_bringup},
    scaffold_deploy::{DeployScaffold, scaffold_deploy},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Project directory to create (project mode), or the literal keyword
    /// `system` to enter Phase 212.F bringup-scaffold mode: `nros new system
    /// <name>_bringup --components <pkg1,pkg2,...>`.
    pub name: Option<PathBuf>,

    /// Phase 212.F bringup-scaffold mode — the bringup package directory.
    /// Only consumed when the first positional is the literal `system`.
    pub system_name: Option<PathBuf>,

    /// Phase 212.F — comma-separated component package names for
    /// `nros new system <bringup> --components <list>`.
    #[arg(long, value_delimiter = ',')]
    pub components: Vec<String>,

    /// Phase 212.F — repeatable single-component form (alternative to
    /// `--components <a,b,c>` when commas in the shell are awkward). Merged
    /// with `--components` at dispatch time.
    #[arg(long = "component-name")]
    pub component_name: Vec<String>,

    /// Phase 212.F — workspace root holding the cargo `Cargo.toml` to
    /// update. Defaults to the parent of the bringup dir.
    #[arg(long)]
    pub workspace_root: Option<PathBuf>,

    /// Phase 212.F — parent dir under which the bringup pkg is created.
    /// Defaults to the current working directory.
    #[arg(long)]
    pub into: Option<PathBuf>,

    /// Phase 212.F — skip the optional `config/` sub-dir.
    #[arg(long)]
    pub no_config: bool,

    /// Phase 212.F — skip the optional `README.md`.
    #[arg(long)]
    pub no_readme: bool,

    /// Target platform (required in project mode)
    #[arg(long, value_parser = ["native", "freertos", "nuttx", "threadx", "zephyr", "posix", "baremetal"])]
    pub platform: Option<String>,

    /// RMW backend
    #[arg(long, value_parser = ["zenoh", "xrce", "cyclonedds"], default_value = "zenoh")]
    pub rmw: String,

    /// Source language
    #[arg(long, value_parser = ["rust", "c", "cpp"], default_value = "rust")]
    pub lang: String,

    /// Use case template
    #[arg(long = "use-case", value_parser = ["talker", "listener", "service", "action"], default_value = "talker")]
    pub use_case: String,

    /// Phase 172 W.3 — scaffold a planned-mode **component** (a reusable
    /// library node with an `nros::Component` + a folded `[component]`
    /// `nros.toml`) instead of a direct-mode binary project. Platform-agnostic
    /// (platform/RMW are chosen at deploy time), so `--platform` is not needed.
    #[arg(long)]
    pub component: bool,

    /// Scaffold a `[deploy.<name>]` target into the bringup package's
    /// `system.toml` (RFC-0004 §4) instead of a project.
    #[arg(long)]
    pub deploy: Option<String>,

    /// Deploy kind (deploy mode) — free-form runner key (`self`, `qemu`,
    /// `flash`, …) written verbatim to `[deploy.<name>].kind`.
    #[arg(long, default_value = "self")]
    pub kind: String,

    /// Cargo target triple / board id / runner key (deploy mode)
    #[arg(long)]
    pub target: Option<String>,

    /// Board (deploy mode)
    #[arg(long)]
    pub board: Option<String>,

    /// Deploy mode: pick the bringup package whose `system.toml` to edit
    /// when the workspace exposes more than one.
    #[arg(long)]
    pub bringup: Option<String>,

    /// Deploy mode: also set the bringup `[system].default_launch` (bootstrap)
    #[arg(long)]
    pub from_launch: Option<String>,

    /// Deploy mode: fork an existing `[deploy.<name>]` profile
    #[arg(long)]
    pub from_profile: Option<String>,

    /// Overwrite an existing directory / `[deploy.<name>]` table
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args) -> Result<()> {
    // Phase 212.F — system / bringup mode: `nros new system <name>_bringup
    // --components <list>`. The literal `system` keyword as the first
    // positional dispatches here.
    if args
        .name
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s == "system")
        .unwrap_or(false)
    {
        let bringup_path = args.system_name.clone().ok_or_else(|| {
            eyre::eyre!("`nros new system <name>_bringup` requires a bringup pkg name")
        })?;
        // Phase 212.F: validate the user-supplied name early so
        // `foo/bar`, `..`, absolute paths surface a clean diagnostic
        // before we touch the filesystem.
        crate::cmd::new_system::validate_bringup_name(&bringup_path)?;
        // Merge --components <a,b,c> with repeatable --component-name <x>.
        let mut components: Vec<String> = args.components.clone();
        components.extend(args.component_name.clone());
        if components.is_empty() {
            bail!(
                "`nros new system <bringup>` requires --components <pkg1,pkg2,...> \
                 (at least one component); --component-name <x> may be repeated as an alternative"
            );
        }
        let cwd = std::env::current_dir()?;
        // --into <dir> overrides cwd as the parent directory for the bringup.
        let into = args.into.clone().unwrap_or_else(|| cwd.clone());
        let bringup_dir = if bringup_path.is_absolute() {
            bringup_path
        } else {
            into.join(&bringup_path)
        };
        let pkg_name = bringup_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| eyre::eyre!("invalid bringup package name"))?
            .to_string();
        let workspace_root = args
            .workspace_root
            .clone()
            .or_else(|| bringup_dir.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| cwd.clone());
        let out = scaffold_bringup(&BringupScaffold {
            bringup_dir: bringup_dir.clone(),
            pkg_name: pkg_name.clone(),
            components: components.clone(),
            workspace_root,
            emit_config: !args.no_config,
            emit_readme: !args.no_readme,
            force: args.force,
        })?;
        eprintln!(
            "nros new system: scaffolded bringup pkg {pkg_name} at {} ({} component(s))",
            out.bringup_dir.display(),
            components.len()
        );
        if let Some(ws) = out.workspace_cargo_toml.as_ref() {
            eprintln!(
                "nros new system: updated [workspace] exclude in {}",
                ws.display()
            );
        }
        let _ = out; // silence unused warning under future changes
        return Ok(());
    }

    // Deploy mode: `nros new --deploy <name> [--kind <k>] ...` writes a
    // `[deploy.<name>]` into the bringup package's `system.toml` (RFC-0004 §4).
    if let Some(deploy_name) = args.deploy {
        return scaffold_deploy(&DeployScaffold {
            name: deploy_name,
            kind: Some(args.kind),
            target: args.target,
            board: args.board,
            from_launch: args.from_launch,
            from_profile: args.from_profile,
            workspace_root: std::env::current_dir()?,
            bringup: args.bringup,
            force: args.force,
        });
    }

    let name = args
        .name
        .as_ref()
        .ok_or_else(|| eyre::eyre!("`nros new <name>` requires a project name"))?
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| eyre::eyre!("invalid project name"))?
        .to_string();

    // Component mode (Phase 172 W.3): a reusable planned-mode library node.
    // Platform-agnostic. Phase 172 W.3 landed Rust; Phase 219.M landed C++;
    // Phase 223 adds the C Node pkg scaffold using the same declarative
    // §212.L.9 shape.
    if args.component {
        match args.lang.as_str() {
            "rust" | "cpp" | "c" => {}
            other => bail!(
                "`nros new --component --lang {other}` is not supported. Use \
                 `rust`, `c`, or `cpp`."
            ),
        }
        return scaffold_component(&ComponentScaffoldConfig {
            name,
            use_case: args.use_case,
            lang: args.lang,
            force: args.force,
        });
    }

    // Project mode.
    let platform = args
        .platform
        .ok_or_else(|| eyre::eyre!("`nros new <name>` requires `--platform <p>`"))?;
    scaffold_package(&ScaffoldConfig {
        name,
        lang: args.lang,
        platform,
        rmw: args.rmw,
        use_case: args.use_case,
        force: args.force,
    })
}
