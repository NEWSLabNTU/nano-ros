//! `nros dev runner check <name>` and `nros dev command check <name>`
//! (RFC-0119 D10, phase-527 W7).
//!
//! The question a package author asks before anyone else runs their package:
//! does nros find it, build it, and talk to it?
//!
//! - **Resolve**: the provider is the one every other lookup picks
//!   (`resolve_runner_with` / `resolve_command_with`, over the same search
//!   path and override policy), so the check fails where `nros run` or
//!   `nros <verb>` would.
//! - **Build**: a package of the workspace is built as `nros build
//!   --packages-select <package>` builds it (the host build role, RFC-0119
//!   D3), by running this `nros`. `--no-build` checks what is already built.
//! - **Executable**: located by the ladder runner lookup and command dispatch
//!   share (`builder::host_tool::locate_built`).
//! - **Protocol**: a runner is asked `describe` once per verb its descriptor
//!   states (`run_exec::check_runner`: the reply parses, shares a protocol
//!   with this nros, implements the verb), and the reply's verbs and needs
//!   must equal the descriptor's. A command has no `describe` in its contract
//!   (RFC-0119 D7 passes its arguments through unchanged), so its
//!   descriptor's protocol is checked against this nros instead, and its
//!   executable is started once with `--help`.
//! - **Plan** (runners): the image's run plan is resolved as `nros run
//!   --dry-run --json [<image>]` resolves it. It must name this runner, and
//!   carry every placeholder the descriptor `needs`. With no `--image` and no
//!   single image, this step is reported as not run, never as passed.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, bail, eyre};
use nros_extension::{
    plan::{RunPlan, Verb},
    protocol,
};

/// `nros dev runner <verb>`.
#[derive(Debug, Subcommand)]
pub enum RunnerSub {
    /// Build a runner package, ask it `describe`, check its protocol and
    /// resolve a dry-run plan through it.
    Check(CheckArgs),
}

/// `nros dev command <verb>`.
#[derive(Debug, Subcommand)]
pub enum CommandSub {
    /// Build a command package and check its descriptor and executable.
    Check(CheckArgs),
}

/// What both checks take.
#[derive(Debug, ClapArgs)]
pub struct CheckArgs {
    /// The runner or command name (the `name` its package announces).
    pub name: String,
    /// Workspace root. Defaults to the current directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// nano-ros checkout (auto-detected if omitted).
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
    /// The image whose run plan to resolve (runners). Omitted: the
    /// workspace's one image, if it has one.
    #[arg(long)]
    pub image: Option<String>,
    /// Approve an override of a nano-ros runner or command by this name.
    #[arg(long, value_name = "NAME")]
    pub allow_overriding: Vec<String>,
    /// Check what is already built; do not run `nros build`.
    #[arg(long)]
    pub no_build: bool,
}

fn workspace_of(a: &CheckArgs) -> PathBuf {
    let ws = a
        .workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    ws.canonicalize().unwrap_or(ws)
}

fn ok(step: &str, what: impl std::fmt::Display) {
    println!("ok    {step:<10} {what}");
}

/// This `nros`, for the steps that run another verb.
fn nros() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| eyre!("locating this nros: {e}"))
}

/// Build `package` the way `nros build --packages-select` does, when it is a
/// package of `ws`.
fn build(a: &CheckArgs, ws: &Path, package: &str, dir: &Path) -> Result<()> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if !dir.starts_with(ws) {
        ok(
            "build",
            format!("skipped: `{package}` is not a package of this workspace"),
        );
        return Ok(());
    }
    if a.no_build {
        ok("build", "skipped (--no-build)");
        return Ok(());
    }
    let mut cmd = Command::new(nros()?);
    cmd.current_dir(ws)
        .args(["build", "--packages-select", package]);
    if let Some(p) = &a.nano_ros_path {
        cmd.arg("--nano-ros-path").arg(p);
    }
    let status = cmd
        .status()
        .map_err(|e| eyre!("running `nros build`: {e}"))?;
    if !status.success() {
        bail!("`nros build --packages-select {package}` failed ({status})");
    }
    ok("build", format!("nros build --packages-select {package}"));
    Ok(())
}

/// `nros dev runner check <name>`.
///
/// # Errors
///
/// The first step that fails, named.
pub fn runner_check(a: CheckArgs) -> Result<()> {
    use cargo_nano_ros::extension_descriptor::resolve_runner_with;
    let ws = workspace_of(&a);
    let root = crate::orchestration::nano_ros_root::resolve(a.nano_ros_path.clone(), &ws);
    let policy = crate::provider_overrides::policy(&ws, &a.allow_overriding)?;
    let path = crate::cmd::ws::provider_search_path_for(&ws, &[])?;
    let scan = cargo_nano_ros::provider_scan::scan_search_path(&path)
        .map_err(|e| eyre!("scanning the provider search path: {e}"))?;
    let r = resolve_runner_with(&scan, &a.name, &policy)
        .map_err(|e| eyre!("runner `{}`: {e}", a.name))?;
    ok(
        "resolve",
        format!(
            "runner `{}` is package `{}` at {}",
            a.name,
            r.package,
            r.dir.display()
        ),
    );
    build(&a, &ws, &r.package, &r.dir)?;
    let located = crate::cmd::run_exec::locate_runner(
        &ws,
        root.as_deref(),
        &a.name,
        &a.allow_overriding,
        &[],
    )
    .map_err(|e| eyre!("{}", e.message))?;
    ok("locate", located.exe.display());

    let mut reply = None;
    for verb in &r.descriptor.verbs {
        let d = crate::cmd::run_exec::check_runner(&located.exe, &a.name, *verb)
            .map_err(|e| eyre!("{e}"))?;
        reply = Some(d);
    }
    let reply = reply.ok_or_else(|| eyre!("runner `{}`'s descriptor states no verb", a.name))?;
    let names = |v: &[Verb]| v.iter().map(|v| v.as_str().to_string()).collect::<Vec<_>>();
    agree(&names(&reply.verbs), &names(&r.descriptor.verbs), "verbs")?;
    agree(&reply.needs, &r.descriptor.needs, "needs")?;
    ok(
        "describe",
        format!(
            "protocol {:?}, verbs {:?}, needs {:?}; equal to nros-runner.toml",
            reply.protocol,
            reply.verbs.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
            reply.needs
        ),
    );

    match dry_run_plan(&a, &ws)? {
        None => println!(
            "NOT RUN plan       this workspace has no single image; name one with --image to \
             resolve a dry-run plan through `{}`",
            a.name
        ),
        // A workspace may hold several runners and an image picks one, so an
        // image that picks another is not a fault in this runner. The plan
        // step is NOT RUN, and says how to point an image at it.
        Some(plan) if plan.runner.name != a.name => println!(
            "NOT RUN plan       image `{}` runs through runner `{}`; to plan it through `{}`, \
             set this in the bringup's system.toml:\n    [image.{}.run]\n    method = \"{}\"",
            plan.image,
            plan.runner.name,
            a.name,
            plan.image.rsplit(':').next().unwrap_or(&plan.image),
            a.name
        ),
        Some(plan) => {
            let missing: Vec<&String> = r
                .descriptor
                .needs
                .iter()
                .filter(|n| plan.placeholder(n).is_none())
                .collect();
            if !missing.is_empty() {
                bail!(
                    "the plan for image `{}` carries no value for {:?}, which nros-runner.toml \
                     `needs`",
                    plan.image,
                    missing
                );
            }
            ok(
                "plan",
                format!(
                    "image `{}` resolves to runner `{}` with every need",
                    plan.image, plan.runner.name
                ),
            );
        }
    }
    println!("runner `{}`: OK", a.name);
    Ok(())
}

fn agree(reply: &[String], stated: &[String], key: &str) -> Result<()> {
    let (mut r, mut s) = (reply.to_vec(), stated.to_vec());
    r.sort();
    s.sort();
    if r != s {
        bail!("`describe` answers {key} {reply:?}, the descriptor states {stated:?}");
    }
    Ok(())
}

/// The run plan `nros run --dry-run --json` resolves, or `None` when no image
/// was named and the workspace does not have exactly one.
fn dry_run_plan(a: &CheckArgs, ws: &Path) -> Result<Option<RunPlan>> {
    let mut cmd = Command::new(nros()?);
    cmd.current_dir(ws).args(["run", "--dry-run", "--json"]);
    if let Some(p) = &a.nano_ros_path {
        cmd.arg("--nano-ros-path").arg(p);
    }
    if let Some(i) = &a.image {
        cmd.arg(i);
    }
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| eyre!("running `nros run --dry-run`: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if a.image.is_none() {
            eprintln!(
                "nros dev runner check: `nros run --dry-run` named no image: {}",
                err.trim()
            );
            return Ok(None);
        }
        bail!(
            "`nros run --dry-run --json {}` failed: {}",
            a.image.as_deref().unwrap_or(""),
            err.trim()
        );
    }
    RunPlan::from_json(&String::from_utf8_lossy(&out.stdout))
        .map(Some)
        .map_err(|e| eyre!("the dry-run plan does not parse: {e}"))
}

/// `nros dev command check <name>`.
///
/// # Errors
///
/// The first step that fails, named.
pub fn command_check(a: CheckArgs) -> Result<()> {
    use cargo_nano_ros::extension_descriptor::resolve_command_with;
    let ws = workspace_of(&a);
    let root = crate::orchestration::nano_ros_root::resolve(a.nano_ros_path.clone(), &ws);
    let policy = crate::provider_overrides::policy(&ws, &a.allow_overriding)?;
    let path = crate::cmd::ws::provider_search_path_for(&ws, &[])?;
    let scan = cargo_nano_ros::provider_scan::scan_search_path(&path)
        .map_err(|e| eyre!("scanning the provider search path: {e}"))?;
    let c = resolve_command_with(&scan, &a.name, &policy)
        .map_err(|e| eyre!("command `{}`: {e}", a.name))?;
    let dev = c.descriptor.tier == cargo_nano_ros::extension_descriptor::CommandTier::Dev;
    if crate::builtin_verbs::taken(&crate::builtin_verbs::tree(), dev).contains(&c.name) {
        bail!(
            "command `{}` is named like a built-in or reserved `nros` verb, so it can never run; \
             rename it",
            c.name
        );
    }
    let spelling = if dev {
        format!("nros dev {}", c.name)
    } else {
        format!("nros {}", c.name)
    };
    ok(
        "resolve",
        format!(
            "`{spelling}` is package `{}` at {}",
            c.package,
            c.dir.display()
        ),
    );
    build(&a, &ws, &c.package, &c.dir)?;
    let exe = crate::builder::host_tool::locate_built(
        &ws,
        root.as_deref(),
        &c.package,
        &c.dir,
        &c.bin,
        &format!("`{spelling}`"),
    )
    .map_err(|e| eyre!("{e}"))?;
    ok("locate", exe.display());
    protocol::negotiate(protocol::SUPPORTED, &c.descriptor.protocol)
        .map_err(|e| eyre!("command `{}`: {e}", c.name))?;
    ok(
        "protocol",
        format!(
            "nros-command.toml states {:?}; this nros speaks {:?}",
            c.descriptor.protocol,
            protocol::SUPPORTED
        ),
    );
    let cli = nros()?;
    let status = Command::new(&exe)
        .arg("--help")
        .env(crate::cmd::extension::ENV_WS_ROOT, &ws)
        .env(
            crate::cmd::extension::ENV_TOOLCHAIN,
            env!("CARGO_PKG_VERSION"),
        )
        .env(crate::cmd::extension::ENV_CLI, OsString::from(cli))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|e| eyre!("starting {}: {e}", exe.display()))?;
    if status.success() {
        ok("start", format!("{} --help exits 0", exe.display()));
    } else {
        // A command owns its arguments (RFC-0119 D7), so `--help` is a
        // courtesy, not part of the contract: report, do not fail.
        println!(
            "note  start      {} --help exited {status}; a command is not required to take \
             --help, but `nros help --all` users will try it",
            exe.display()
        );
    }
    println!("command `{}`: OK", c.name);
    Ok(())
}
