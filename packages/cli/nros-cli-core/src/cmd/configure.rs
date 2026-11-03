//! `nros configure` (phase-530 W1, RFC-0120 D2) -- and `nros sync`, its alias.
//!
//! Configure is C1 (what `nros sync` did: message codegen, the metadata
//! probe, models, the leaf lock seed) plus C2-C6 (`nros build` stages 1-4:
//! discover, preflight, census, resolve, generate), under one per-project
//! lock. It never runs a target build tool and never calls `nros build`
//! (RFC-0120 D1): stage 4 is [`build::plan_builds`], which writes the
//! generated files and spawns nothing.
//!
//! Each image that planned records a stamp (`crate::configure_stamp`), so a
//! rerun with nothing changed is a no-op costing a digest comparison.

use std::path::{Path, PathBuf};

use clap::Parser;
use eyre::{Result, WrapErr};
use nros_orchestration_ir::leaf_system;

use crate::{
    builder::preflight::PreflightRefusal,
    cmd::{build, ws},
    configure_stamp::{self, Currency},
    file_lock::lock_exclusive_path,
};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(flatten)]
    pub sync: ws::SyncArgs,

    /// Configure even when every image's stamp is current.
    #[arg(long)]
    pub force: bool,
}

/// What one configure did. `run` prints it; tests read it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Every image's stamp was current, so nothing ran.
    pub up_to_date: bool,
    /// Stamp keys of the images that planned and recorded a stamp.
    pub configured: Vec<String>,
    /// `(stamp key, reason)` of each image whose preflight refused it and
    /// which the caller did not name. No stamp is recorded for these.
    pub skipped: Vec<(String, String)>,
    /// `(stamp key, error)` of each image the caller did not name whose
    /// stages 2-4 failed for a reason other than a preflight refusal (Ruling
    /// R22). No stamp is recorded, so the next configure tries it again.
    pub not_configured: Vec<(String, String)>,
    /// How many times C1 (`ws::run_sync`) ran, counted where it starts. One
    /// when anything ran: a configure that must sync twice to converge is a
    /// defect (phase-530 W1, Ruling R10).
    pub sync_passes: usize,
}

/// One image to configure: the key its stamp lives under, the name
/// `plan_builds` is asked for, and whether the caller named it.
struct Target {
    key: String,
    select: String,
    named: bool,
}

pub fn run(args: Args) -> Result<()> {
    configure(args).map(|_| ())
}

/// [`run`], returning what it did.
pub fn configure(args: Args) -> Result<Outcome> {
    let project = project_root(args.sync.workspace.as_deref())?;
    // `--check` and `--dry-run` keep the meaning they had on `nros sync`: a
    // check, and a print of what would be written. Neither writes, so neither
    // plans nor records a stamp.
    if args.sync.check || args.sync.dry_run {
        ws::run_sync(args.sync)?;
        return Ok(Outcome::default());
    }
    let named: Vec<String> = args.sync.image.iter().cloned().collect();
    configure_project(&project, args.sync, &named, args.force, false)
}

/// Configure `project` for `images` (empty: every image the project
/// declares) unless every one is already current. `nros build` and `nros run`
/// call this before they plan (RFC-0120 D3); `dry_run` is theirs, handed to
/// stage 4 unchanged.
pub fn ensure_current(
    project: &Path,
    images: &[String],
    dry_run: bool,
    offline: bool,
) -> Result<()> {
    // phase-530 W2 (RFC-0120 D1) retired ~~`NROS_CONFIGURE_HELD`~~: nothing a
    // configure runs calls back into `nros build` or `nros configure` any
    // more. The census builds its host image through the host tool, in this
    // process, and its build sets `NROS_LEAF_CENSUS_HOST`, which every hook
    // honours; so no child waits on the lock this configure holds.
    let project = project_root(Some(project))?;
    let mut argv = vec![
        "configure".to_string(),
        project.to_string_lossy().into_owned(),
    ];
    // A leaf's sync generates for ONE image; a workspace's sync covers every
    // image, and `--image` means nothing to it.
    if is_leaf(&project)
        && let [one] = images
    {
        argv.push("--image".to_string());
        argv.push(bare_id(one).to_string());
    }
    if offline {
        argv.push("--offline".to_string());
    }
    let sync = SyncOnly::try_parse_from(&argv)
        .wrap_err("configure: building the sync arguments")?
        .sync;
    configure_project(&project, sync, images, false, dry_run).map(|_| ())
}

/// `SyncArgs` with every default, the way the parser would build them.
pub(crate) fn default_sync_args() -> ws::SyncArgs {
    SyncOnly::try_parse_from(["configure"])
        .expect("`configure` with no flags parses")
        .sync
}

/// `SyncArgs` alone, for the parser.
#[derive(Parser)]
struct SyncOnly {
    #[command(flatten)]
    sync: ws::SyncArgs,
}

/// The shared body of [`configure`] and [`ensure_current`].
fn configure_project(
    project: &Path,
    sync: ws::SyncArgs,
    named: &[String],
    force: bool,
    plan_dry_run: bool,
) -> Result<Outcome> {
    ensure_interface_search_path(project);
    let flags = configure_stamp::OutputFlags::of(&sync);
    let _lock = lock_exclusive_path(&configure_stamp::lock_path(project))?;

    let targets = targets(project, named)?;
    // Ruling R15: a project that declares no image (a bringup-less workspace)
    // still has inputs C1 reads, so it records ONE project-level stamp.
    let keys: Vec<(&str, bool)> = if targets.is_empty() {
        vec![(configure_stamp::PROJECT_KEY, false)]
    } else {
        targets.iter().map(|t| (t.key.as_str(), t.named)).collect()
    };
    if !force {
        let mut all_current = true;
        for (key, named) in &keys {
            if let Currency::Stale(why) = configure_stamp::is_current(project, key, *named, &flags)?
            {
                eprintln!("configure: {key}: {why}");
                all_current = false;
                break;
            }
        }
        if all_current {
            println!("configure: up to date ({} image(s))", targets.len());
            return Ok(Outcome {
                up_to_date: true,
                ..Outcome::default()
            });
        }
    }

    // ---- C1: what `nros sync` did -------------------------------------
    let nano_ros_path = sync.nano_ros_path.clone();
    let offline = sync.offline;
    // Ruling R9/R14: which image holds a leaf's shared output. Its writer,
    // `leaf_settings::write_selection`, records it for a leaf that has a
    // selection package (so `nros build <other>` moves it too); configure
    // records it as well, because a leaf without one still shares
    // `generated/`, and before C1 writes the first byte.
    if is_leaf(project)
        && let [t] = targets.as_slice()
    {
        configure_stamp::record_leaf_selection(project, &t.key)?;
    }
    let syncs_before = ws::sync_runs();
    ws::run_sync(sync)?;

    // ---- C2-C6: `nros build` stages 1-4, one image at a time ----------
    let mut out = Outcome::default();
    let mut resolved: std::collections::BTreeMap<String, PathBuf> = Default::default();
    for t in &targets {
        match build::plan_builds(&build_args(
            project,
            &t.select,
            &nano_ros_path,
            plan_dry_run,
            offline,
        )) {
            Ok(builds) => {
                if let Some(d) = builds.iter().find_map(|b| resolve_dir_of(project, b)) {
                    resolved.insert(t.key.clone(), d);
                }
                out.configured.push(t.key.clone())
            }
            Err(e) if t.named => {
                return Err(e.wrap_err(format!("configure: image `{}`", t.key)));
            }
            Err(e) => match e.downcast_ref::<PreflightRefusal>() {
                Some(refusal) => {
                    let why = refusal.summary();
                    eprintln!("configure: {}: skipped — {why}", t.key);
                    out.skipped.push((t.key.clone(), why));
                }
                // Ruling R22: an image nobody named that fails stages 2-4 for
                // any other reason is reported and left unconfigured, with no
                // stamp, so the next configure tries it again. C1 succeeded,
                // which is what `nros sync` exited 0 on; `nros build` of that
                // image then fails on its own error.
                None => {
                    let chain = format!("{e:#}").lines().collect::<Vec<_>>().join(" ");
                    eprintln!("configure: {}: not configured — {chain}", t.key);
                    out.not_configured.push((t.key.clone(), chain));
                }
            },
        }
    }

    // ---- the stamps: inputs as they are AFTER this configure wrote -----
    // Ruling R16: a dry run's stage 4 wrote nothing, so it records nothing —
    // the rule `--check`/`--dry-run` already follow.
    if !plan_dry_run {
        let stamp = configure_stamp::stamp_now(project, &flags)?;
        let project_wide = targets
            .is_empty()
            .then(|| configure_stamp::PROJECT_KEY.to_string());
        for key in out.configured.iter().chain(project_wide.iter()) {
            let stamp = configure_stamp::Stamp {
                resolved_dir: resolved.get(key).cloned(),
                ..stamp.clone()
            };
            configure_stamp::record(project, key, &stamp)?;
        }
        // Ruling R15: a skipped image records the digest and its reason, so
        // an unchanged project does not reconfigure on every build.
        for (key, why) in &out.skipped {
            let skip = configure_stamp::Stamp {
                skipped: Some(why.clone()),
                ..stamp.clone()
            };
            configure_stamp::record(project, key, &skip)?;
        }
    }
    out.sync_passes = ws::sync_runs() - syncs_before;
    Ok(out)
}

/// The `build::Args` stage 4 runs with: one image, nothing else selected
/// (the template is `materialize.rs`'s literal).
pub(crate) fn build_args(
    project: &Path,
    image: &str,
    nano_ros_path: &Option<PathBuf>,
    dry_run: bool,
    offline: bool,
) -> build::Args {
    build::Args {
        images: vec![image.to_string()],
        workspace: Some(project.to_path_buf()),
        nano_ros_path: nano_ros_path.clone(),
        // Stage 4 never runs west, so no Zephyr is needed and none is looked for.
        zephyr_workspace: None,
        board: None,
        all: false,
        dry_run,
        verbose: false,
        offline,
        selection: Default::default(),
        continue_on_error: false,
        cmake_args: Vec::new(),
        parallel_workers: None,
        build_base: None,
        check: None,
        native_args: Vec::new(),
    }
}

/// phase-530 W4 (RFC-0120 D2) — the directory stage 3.5 resolved `build`'s
/// image into (`build/<bringup>__<id>/`, [`crate::resolve::dir_name_for`]),
/// when it wrote one. Recorded in the image's stamp, so a host configure
/// locates it by the image's identity (`nros locate configured`).
fn resolve_dir_of(project: &Path, build: &build::ResolvedBuild) -> Option<PathBuf> {
    let (bringup, id) = build.qualified.split_once(':')?;
    let dir = project
        .join("build")
        .join(crate::resolve::dir_name_for(bringup, id));
    dir.join(crate::resolve::RESOLVED_TOML_NAME)
        .is_file()
        .then_some(dir)
}

/// The project: `dir`, else the working directory, canonical.
fn project_root(dir: Option<&Path>) -> Result<PathBuf> {
    let dir = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_dir().wrap_err("configure: resolving cwd")?,
    };
    std::fs::canonicalize(&dir).wrap_err_with(|| format!("configure: {}", dir.display()))
}

/// The ONE place configure writes the process environment: `Some` sets
/// `var`, `None` removes it. Every writer serialises on one mutex.
fn set_env(var: &str, value: Option<&std::ffi::OsStr>) {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // SAFETY: edition 2024 makes these unsafe because another thread may be
    // reading the environment. Configure's writers (the interface search
    // path) all come through here and are serialised on
    // ENV_LOCK; the CLI runs configure on one thread, and the readers are
    // this CLI's own verbs and the processes configure spawns from that
    // thread, which inherit a consistent copy.
    unsafe {
        match value {
            Some(v) => std::env::set_var(var, v),
            None => std::env::remove_var(var),
        }
    }
}

/// A single-package leaf: a `system.toml` beside a package manifest.
fn is_leaf(project: &Path) -> bool {
    project.join(leaf_system::SYSTEM_TOML).is_file() && leaf_system::is_package_dir(project)
}

/// `linux` for `pkg:linux` or `linux`.
fn bare_id(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The images to configure, enumerated WITHOUT preflight (Ruling R3).
///
/// A leaf builds one image per invocation (issue 1305: its images share one
/// generated selection package and one `generated/` tree), so a leaf's list
/// is the image named, else the leaf's own rule (one image, or `[system]
/// default_images`). A workspace's is every image its bringups declare, or
/// the ones named.
fn targets(project: &Path, named: &[String]) -> Result<Vec<Target>> {
    if is_leaf(project) {
        let decl = match named {
            [] => leaf_system::read(project),
            [one] => leaf_system::read_image(project, bare_id(one)),
            many => eyre::bail!(
                "{} is a single-package leaf: configure one image at a time (asked for {})",
                project.display(),
                many.join(", ")
            ),
        }
        .map_err(|e| eyre::eyre!(e))?;
        let Some(decl) = decl else {
            return Ok(Vec::new());
        };
        let Some(id) = decl.image.clone().or_else(|| decl.board.clone()) else {
            return Ok(Vec::new());
        };
        return Ok(vec![Target {
            key: id.clone(),
            select: id,
            named: !named.is_empty(),
        }]);
    }
    let all = build::workspace_images(project)?;
    if named.is_empty() {
        return Ok(all
            .into_iter()
            .map(|q| Target {
                key: q.clone(),
                select: q,
                named: false,
            })
            .collect());
    }
    let mut out = Vec::new();
    for want in named {
        let hits: Vec<&String> = all
            .iter()
            .filter(|q| *q == want || bare_id(q) == want.as_str())
            .collect();
        match hits.as_slice() {
            [one] => out.push(Target {
                key: (*one).clone(),
                select: (*one).clone(),
                named: true,
            }),
            [] => eyre::bail!(
                "configure: no image `{want}` in {} (declared: {})",
                project.display(),
                all.join(", ")
            ),
            many => eyre::bail!(
                "configure: `{want}` names several images ({}); qualify it as `<bringup>:<id>`",
                many.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    Ok(out)
}

/// RFC-0120 D2: configure computes the interface search path itself, which
/// retires `eval "$(nros dev ws env)"`. A caller's own path wins: one set before
/// the process started is left alone. A project with no package layout has none.
///
/// Derived per PROJECT on every call (Ruling R12): a process that configures
/// two projects must not hand the second the first one's path. Whether the
/// USER set it is captured once, at the first call, before this function has
/// ever written it.
fn ensure_interface_search_path(project: &Path) {
    const VAR: &str = "NROS_INTERFACE_SEARCH_PATH";
    static USER_SET: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *USER_SET.get_or_init(|| std::env::var_os(VAR).is_some()) {
        return;
    }
    let root = ws::interface_search_root(project).ok();
    set_env(VAR, root.as_ref().map(|r| r.as_os_str()));
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn bare_id_strips_one_qualifier() {
        assert_eq!(bare_id("linux"), "linux");
        assert_eq!(bare_id("demo_bringup:linux"), "linux");
    }

    #[test]
    fn a_preflight_refusal_survives_eyre_and_downcasts() {
        let m = crate::builder::preflight::Missing {
            what: "Rust target `x`".into(),
            remedy: "rustup target add x".into(),
        };
        let e: eyre::Report = PreflightRefusal(vec![m.clone()]).into();
        let e = e.wrap_err("context");
        let r = e
            .downcast_ref::<PreflightRefusal>()
            .expect("typed through wrap_err");
        assert_eq!(
            r.summary(),
            "missing Rust target `x` (run: rustup target add x)"
        );
        // The message is stage 3's report, unchanged.
        assert_eq!(
            PreflightRefusal(vec![m.clone()]).to_string(),
            crate::builder::preflight::report(&[m])
        );
    }
}
