//! `nros check <package>` (issue 2294), and its aliases `nros dev runner
//! check <name>` and `nros dev command check <name>` (RFC-0119 D10,
//! phase-527 W7; amended 2026-11-07).
//!
//! [`package_check`] works out what a package is (a runner, a command, a
//! board) and runs that kind's check; a board's is `board::board_check`.
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

/// What a package checked by name is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Runner,
    Command,
    Board,
}

impl PackageKind {
    fn from_provision(kind: &str) -> Option<Self> {
        match kind {
            "runner" => Some(Self::Runner),
            "command" => Some(Self::Command),
            "board" => Some(Self::Board),
            _ => None,
        }
    }
}

/// The checks `name` asks for: one `(kind, provision name)` per runner,
/// command or board that the name answers to, in that order.
///
/// A provision answers to its own name and to its package's name, so
/// `nros check nros_board_acme` and `nros check acme` both find the board
/// the scaffold wrote. A board is also any catalog descriptor that answers
/// to `name` (an alias), or whose `package.xml` is named `name`: a board on
/// `$NROS_EXTRA_BOARD_PATH` is in the catalog and not in the provider scan.
/// `root` is the SDK root the catalog's in-tree sources are relative to.
pub fn kinds_of(
    scan: &cargo_nano_ros::provider_scan::ScanResult,
    boards: &crate::orchestration::board_descriptor::BoardCatalog,
    root: &Path,
    name: &str,
) -> Vec<(PackageKind, String)> {
    let mut out: Vec<(PackageKind, String)> = Vec::new();
    for p in &scan.providers {
        for pr in &p.provides {
            let Some(kind) = PackageKind::from_provision(&pr.kind) else {
                continue;
            };
            if pr.name != name && p.package != name {
                continue;
            }
            // A board package announces every name of its boards; one check
            // covers the descriptor they share.
            if kind == PackageKind::Board && out.iter().any(|(k, _)| *k == kind) {
                continue;
            }
            let row = (kind, pr.name.clone());
            if !out.contains(&row) {
                out.push(row);
            }
        }
    }
    if !out.iter().any(|(k, _)| *k == PackageKind::Board) {
        if boards.descriptors().iter().any(|d| d.answers_to(name)) {
            out.push((PackageKind::Board, name.to_string()));
        } else if let Some(key) = boards.descriptors().iter().find_map(|d| {
            let dir = crate::orchestration::board_projection::descriptor_dir(d, root)?;
            let scan = cargo_nano_ros::provider_scan::scan_root(&dir, 0).ok()?;
            let named = scan
                .providers
                .iter()
                .any(|p| p.dir == dir && p.package == name);
            if named {
                d.names.first().cloned()
            } else {
                None
            }
        }) {
            out.push((PackageKind::Board, key));
        }
    }
    out.sort_by_key(|(k, _)| *k as u8);
    out
}

/// `nros check <package>`: work out the package's kind and run its check.
///
/// # Errors
///
/// No runner, command or board answers to the name, or a check fails.
pub fn package_check(a: CheckArgs) -> Result<()> {
    let ws = workspace_of(&a);
    let root = crate::orchestration::nano_ros_root::resolve(a.nano_ros_path.clone(), &ws);
    let path = crate::cmd::ws::provider_search_path_for(&ws, &[])?;
    let scan = cargo_nano_ros::provider_scan::scan_search_path(&path)
        .map_err(|e| eyre!("scanning the provider search path: {e}"))?;
    let boards = match &root {
        Some(r) => crate::cmd::board::workspace_catalog(r, &ws)?,
        None => Default::default(),
    };
    let kinds = kinds_of(&scan, &boards, root.as_deref().unwrap_or(&ws), &a.name);
    if kinds.is_empty() {
        bail!(
            "nothing here is named `{name}`: no file or directory `{name}`, and no runner, \
             command or board package answers to it.\n  \
             Run it from the workspace root. `nros list` names the workspace's packages, \
             `nros board list` the boards; an out-of-tree board is reached through \
             $NROS_EXTRA_BOARD_PATH.",
            name = a.name
        );
    }
    for (kind, name) in kinds {
        let one = CheckArgs {
            name,
            workspace: Some(ws.clone()),
            nano_ros_path: a.nano_ros_path.clone(),
            image: a.image.clone(),
            allow_overriding: a.allow_overriding.clone(),
            no_build: a.no_build,
        };
        match kind {
            PackageKind::Runner => runner_check(one)?,
            PackageKind::Command => command_check(one)?,
            PackageKind::Board => {
                let root = root.as_deref().ok_or_else(|| {
                    eyre!("{}", crate::orchestration::nano_ros_root::not_found_help())
                })?;
                crate::cmd::board::board_check(root, &ws, &one.name)?;
            }
        }
    }
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
                "nros check: `nros run --dry-run` named no image: {}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::board_descriptor::BoardCatalog;

    fn pkg(root: &Path, dir: &str, name: &str, provides: &[(&str, &str)]) -> PathBuf {
        let d = root.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        let tags: String = provides
            .iter()
            .map(|(k, n)| format!("<nano_ros_provides kind=\"{k}\" name=\"{n}\"/>"))
            .collect();
        std::fs::write(
            d.join("package.xml"),
            format!(
                "<?xml version=\"1.0\"?><package format=\"3\"><name>{name}</name>\
                 <version>0.1.0</version><description>x</description>\
                 <maintainer email=\"a@b.c\">a</maintainer><license>MIT</license>\
                 <export>{tags}</export></package>"
            ),
        )
        .unwrap();
        d
    }

    /// Issue 2294 — one name, whatever kind its package is: a runner and a
    /// command by provision or package name, a board by its key, its package
    /// or a catalog alias; nothing for a name nothing answers to.
    #[test]
    fn a_name_finds_the_kind_of_its_package() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        pkg(
            ws,
            "src/acme_flash",
            "acme_flash",
            &[("runner", "acme_flash")],
        );
        pkg(ws, "src/tools_pkg", "tools_pkg", &[("command", "report")]);
        pkg(ws, "src/msgs", "msgs", &[("rmw", "msgs")]);
        let board = pkg(
            ws,
            "src/nros-board-acme",
            "nros_board_acme",
            &[("board", "acme"), ("board", "acme-alt")],
        );
        std::fs::write(
            board.join("nros-board.toml"),
            "[[board]]\nnames = [\"acme\", \"acme-alt\", \"acme-old\"]\nplatform = \"posix\"\n\
             toolchain = \"stable\"\nplatform_feature = \"platform-posix\"\nentry_kind = \"hosted-main\"\n",
        )
        .unwrap();
        let scan = cargo_nano_ros::provider_scan::scan_roots(&[ws.to_path_buf()]).unwrap();
        let sdk = tmp.path().join("sdk");
        std::fs::create_dir_all(sdk.join("packages/boards")).unwrap();
        let boards = BoardCatalog::load_with_extra(&sdk, &[ws.join("src")]).unwrap();

        let k = |n: &str| kinds_of(&scan, &boards, &sdk, n);
        assert_eq!(
            k("acme_flash"),
            [(PackageKind::Runner, "acme_flash".into())]
        );
        assert_eq!(k("report"), [(PackageKind::Command, "report".into())]);
        assert_eq!(k("tools_pkg"), [(PackageKind::Command, "report".into())]);
        assert_eq!(k("acme"), [(PackageKind::Board, "acme".into())]);
        assert_eq!(k("nros_board_acme"), [(PackageKind::Board, "acme".into())]);
        // An alias the descriptor holds and the announcement does not.
        assert_eq!(k("acme-old"), [(PackageKind::Board, "acme-old".into())]);
        // A board the catalog holds and the scan does not (an extra board
        // root), by its package name.
        let empty = cargo_nano_ros::provider_scan::ScanResult::default();
        assert_eq!(
            kinds_of(&empty, &boards, &sdk, "nros_board_acme"),
            [(PackageKind::Board, "acme".into())]
        );
        // Another kind of provider, and nothing at all, ask for no check.
        assert!(k("msgs").is_empty());
        assert!(k("nobody").is_empty());
    }

    /// A package that is both a runner and a command gets both checks,
    /// runner first.
    #[test]
    fn a_package_of_two_kinds_gets_both_checks() {
        let tmp = tempfile::tempdir().unwrap();
        pkg(
            tmp.path(),
            "src/both",
            "both",
            &[("command", "both"), ("runner", "both")],
        );
        let scan = cargo_nano_ros::provider_scan::scan_roots(&[tmp.path().to_path_buf()]).unwrap();
        assert_eq!(
            kinds_of(&scan, &BoardCatalog::default(), tmp.path(), "both"),
            [
                (PackageKind::Runner, "both".into()),
                (PackageKind::Command, "both".into())
            ]
        );
    }
}
