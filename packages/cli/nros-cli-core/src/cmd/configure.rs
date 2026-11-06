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
    configure_with(args, std::env::var_os(HELD_VAR).as_deref())
}

/// [`configure`] with [`HELD_VAR`]'s value injected.
pub(crate) fn configure_with(args: Args, held: Option<&std::ffi::OsStr>) -> Result<Outcome> {
    let project = project_root(args.sync.workspace.as_deref())?;
    // Ruling R25: only `ensure_current` (the implicit configure of a child a
    // configure spawned) honours the marker. An EXPLICIT configure under it
    // was asked to do work; succeeding without doing any would let a CI
    // `nros configure --check` pass vacuously.
    if held_by_parent(&project, held) {
        eyre::bail!(
            "{HELD_VAR} is internal to nros configure (it marks a configure already \
             running for this project); unset it"
        );
    }
    // `--check` and `--dry-run` keep the meaning they had on `nros sync`: a
    // check, and a print of what would be written. Neither writes, so neither
    // plans nor records a stamp.
    if args.sync.check || args.sync.dry_run {
        ws::run_sync(args.sync)?;
        return Ok(Outcome::default());
    }
    let named: Vec<String> = args.sync.image.iter().cloned().collect();
    configure_project(&project, args.sync, &named, args.force)
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
    ensure_current_with(
        project,
        images,
        dry_run,
        offline,
        std::env::var_os(HELD_VAR).as_deref(),
    )
}

/// The environment variable a configure exports while it holds a project's
/// lock: the canonical project path.
///
/// INTERNAL: never set it by hand. Only the implicit configure a child of
/// that configure runs (`ensure_current`) honours it; an explicit `nros
/// configure` under it for the same project fails (Ruling R25).
pub(crate) const HELD_VAR: &str = "NROS_CONFIGURE_HELD";

/// [`ensure_current`] with [`HELD_VAR`]'s value injected, so a test need not
/// mutate the process environment.
pub(crate) fn ensure_current_with(
    project: &Path,
    images: &[String],
    dry_run: bool,
    offline: bool,
    held: Option<&std::ffi::OsStr>,
) -> Result<()> {
    let project = project_root(Some(project))?;
    // Ruling R20: a parent configure of THIS project holds its lock and is
    // what spawned us (stage 3.4's census runs `nros build`). It is already
    // configuring; waiting on its lock would deadlock both.
    if held_by_parent(&project, held) {
        return Ok(());
    }
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
    // Ruling R43: a dry run never configures, so it writes nothing (no C1
    // codegen, model, lock seed or stamp). It says what a real run would do,
    // and the caller plans from the project as it stands.
    if dry_run {
        for (key, why) in would_configure(&project, &sync, images)? {
            eprintln!("nros build: --dry-run: would configure {key}: {why}");
        }
        return Ok(());
    }
    configure_project(&project, sync, images, false).map(|_| ())
}

/// Ruling R43 — the `(stamp key, reason)` of every image a configure of
/// `project` would run for, by the same currency check, writing nothing.
fn would_configure(
    project: &Path,
    sync: &ws::SyncArgs,
    named: &[String],
) -> Result<Vec<(String, String)>> {
    let flags = configure_stamp::OutputFlags::of(sync);
    let targets = targets(project, named)?;
    let keys: Vec<(String, bool)> = if targets.is_empty() {
        vec![(configure_stamp::PROJECT_KEY.to_string(), false)]
    } else {
        targets.iter().map(|t| (t.key.clone(), t.named)).collect()
    };
    let mut out = Vec::new();
    for (key, named) in keys {
        if let Currency::Stale(why) = configure_stamp::is_current(project, &key, named, &flags)? {
            out.push((key, why));
        }
    }
    Ok(out)
}

/// `SyncArgs` with every default, the way the parser would build them.
#[cfg(test)]
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
    mut sync: ws::SyncArgs,
    named: &[String],
    force: bool,
) -> Result<Outcome> {
    // Ruling R45: the interface search path is an ARGUMENT, derived once
    // here, and never read from or written to the process environment.
    sync.interface_search_path = interface_search_path_for(project);
    let flags = configure_stamp::OutputFlags::of(&sync);
    let _lock = lock_exclusive_path(&configure_stamp::lock_path(project))?;
    // Declared AFTER `_lock`, so it drops first: the marker never outlives
    // the lock it announces.
    let _held = HeldMarker::hold(project);

    let targets = targets(project, named)?;
    // Ruling R15: a project that declares no image (a bringup-less workspace)
    // still has inputs C1 reads, so it records ONE project-level stamp.
    let keys: Vec<(&str, bool)> = if targets.is_empty() {
        vec![(configure_stamp::PROJECT_KEY, false)]
    } else {
        targets.iter().map(|t| (t.key.as_str(), t.named)).collect()
    };
    // RFC-0120 D2 (Ruling R32): before the currency check AND C1, so both
    // see the environment this configure runs with — the recorded one where
    // the caller's leaves it unset.
    let stamp_keys: Vec<&str> = keys.iter().map(|(k, _)| *k).collect();
    for (var, value) in recorded_env_to_restore_with(
        project,
        sync.interface_search_path.as_deref(),
        &stamp_keys,
        std::env::var_os(configure_stamp::AMENT_VAR).is_some(),
        std::env::var_os(configure_stamp::LAUNCH_RESOLVE_VAR).is_some()
            || configure_stamp::current_launch_resolver().is_some(),
    )? {
        set_env(var, Some(value.as_os_str()));
    }
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
    for t in &targets {
        match build::plan_builds(&build_args(
            project,
            &t.select,
            &nano_ros_path,
            false,
            offline,
        )) {
            Ok(_) => out.configured.push(t.key.clone()),
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
    {
        let stamp = configure_stamp::stamp_now(project, &flags)?;
        let project_wide = targets
            .is_empty()
            .then(|| configure_stamp::PROJECT_KEY.to_string());
        for key in out.configured.iter().chain(project_wide.iter()) {
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

/// RFC-0120 D2 (Ruling R32) — the environment travels with the stamp. What
/// a configure of `project` must restore, in-process, from the first stamp
/// recorded under `keys`, for a caller whose environment lacks it (an
/// IDE-launched host configure has no sourced ROS shell):
///
/// - `AMENT_PREFIX_PATH` unset (`ament_set` false) → the recorded prefix;
/// - no `nros-launch-resolve` resolvable (`resolver_found` false) → the
///   recorded path, through the ladder's explicit rung, if it still exists.
///
/// With `AMENT_PREFIX_PATH` unset and NO stamp recorded at all there is
/// nothing to reuse, so configure refuses, naming the setup step. A stamp
/// that recorded no prefix (a run that had none) is a record: nothing to
/// restore, and no refusal.
#[cfg(test)]
pub(crate) fn recorded_env_to_restore(
    project: &Path,
    keys: &[&str],
    ament_set: bool,
    resolver_found: bool,
) -> Result<Vec<(&'static str, std::ffi::OsString)>> {
    recorded_env_to_restore_with(
        project,
        interface_search_path_for(project).as_deref(),
        keys,
        ament_set,
        resolver_found,
    )
}

/// [`recorded_env_to_restore`] with the interface search path the run uses.
pub(crate) fn recorded_env_to_restore_with(
    project: &Path,
    search: Option<&std::ffi::OsStr>,
    keys: &[&str],
    ament_set: bool,
    resolver_found: bool,
) -> Result<Vec<(&'static str, std::ffi::OsString)>> {
    if ament_set && resolver_found {
        return Ok(Vec::new());
    }
    let Some(stamp) = keys.iter().find_map(|k| configure_stamp::read(project, k)) else {
        // Ruling R44: only a C1 with a message package to generate needs
        // ament; a project with none configures on any host.
        if !ament_set && ws::has_messages_to_generate(project, search) {
            // RFC-0120 D2: no record → refuse, naming the setup step.
            eyre::bail!(
                "configure: `{}` is unset and no configure of `{}` has recorded one to reuse. \
                 Run the setup step once from a shell that has ROS 2: \
                 `source /opt/ros/<distro>/setup.bash` (or your overlay's `install/setup.bash`), \
                 then `nros configure`; later runs without a sourced shell reuse what it records.",
                configure_stamp::AMENT_VAR,
                project.display()
            );
        }
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    if !ament_set && let Some(prefix) = stamp.ament_prefix {
        out.push((configure_stamp::AMENT_VAR, prefix.into()));
    }
    if !resolver_found && let Some(resolver) = stamp.launch_resolve.filter(|r| r.is_file()) {
        out.push((
            configure_stamp::LAUNCH_RESOLVE_VAR,
            resolver.into_os_string(),
        ));
    }
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
        jobs: None,
        parallel_workers: None,
        build_base: None,
        check: None,
        allow_overriding: Vec::new(),
        native_args: Vec::new(),
    }
}

/// The project: `dir`, else the working directory, canonical.
fn project_root(dir: Option<&Path>) -> Result<PathBuf> {
    let dir = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_dir().wrap_err("configure: resolving cwd")?,
    };
    std::fs::canonicalize(&dir).wrap_err_with(|| format!("configure: {}", dir.display()))
}

/// Whether `marker` ([`HELD_VAR`]'s value) names `project`: a parent
/// configure holds its lock. Compared canonically.
fn held_by_parent(project: &Path, marker: Option<&std::ffi::OsStr>) -> bool {
    let Some(marker) = marker.filter(|m| !m.is_empty()) else {
        return false;
    };
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(Path::new(marker)) == canon(project)
}

/// Ruling R20: the projects whose configure lock THIS process holds, for as
/// long as the value lives. In memory, never in the process environment
/// (Ruling R45): the one child that needs it — the census `nros build` a
/// configure spawns — is handed [`HELD_VAR`] on its own command
/// ([`held_marker_for`]), so a configure on another thread of the same
/// process never sees it.
struct HeldMarker(PathBuf);

fn held_projects() -> std::sync::MutexGuard<'static, Vec<PathBuf>> {
    static HELD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    HELD.lock().unwrap_or_else(|p| p.into_inner())
}

impl HeldMarker {
    fn hold(project: &Path) -> Self {
        held_projects().push(project.to_path_buf());
        HeldMarker(project.to_path_buf())
    }
}

impl Drop for HeldMarker {
    fn drop(&mut self) {
        let mut held = held_projects();
        if let Some(i) = held.iter().position(|p| *p == self.0) {
            held.remove(i);
        }
    }
}

/// `(HELD_VAR, project)` for a child process this process spawns for
/// `project` while it holds that project's configure lock, else `None`.
pub(crate) fn held_marker_for(project: &Path) -> Option<(&'static str, std::ffi::OsString)> {
    let canon = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    held_projects()
        .contains(&canon)
        .then(|| (HELD_VAR, canon.into_os_string()))
}

/// The ONE place configure writes the process environment: `Some` sets
/// `var`, `None` removes it. Every writer serialises on one mutex. Its only
/// writer is the recorded environment a stamp restores for a caller that
/// lacks it (RFC-0120 D2, Ruling R32): `AMENT_PREFIX_PATH` and
/// `NROS_LAUNCH_RESOLVE`, read by C1's ament index and every child it spawns.
pub(crate) fn set_env(var: &str, value: Option<&std::ffi::OsStr>) {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // SAFETY: edition 2024 makes these unsafe because another thread may be
    // reading the environment. Configure's one writer (the recorded
    // environment) comes through here and is serialised on ENV_LOCK; the
    // CLI runs configure on one thread, and the readers are this CLI's own
    // verbs and the processes configure spawns from that thread, which
    // inherit a consistent copy.
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

const INTERFACE_SEARCH_VAR: &str = "NROS_INTERFACE_SEARCH_PATH";

/// The USER's `NROS_INTERFACE_SEARCH_PATH`: the one read of the variable,
/// once, at the edge. Configure never writes it (Ruling R45).
pub(crate) fn user_interface_search_path() -> Option<&'static std::ffi::OsString> {
    static USER: std::sync::OnceLock<Option<std::ffi::OsString>> = std::sync::OnceLock::new();
    USER.get_or_init(|| std::env::var_os(INTERFACE_SEARCH_VAR))
        .as_ref()
}

/// THE default `NROS_INTERFACE_SEARCH_PATH` of `project` (Ruling R12): its
/// interface root. One derivation for the value configure hands sync and for
/// the configure stamp's digest of an unset variable
/// (`configure_stamp::input_digest_with`), so the stamp's writer, a configure
/// that checks it, and a reader that runs no configure (`nros locate
/// configured`) all digest the same project alike.
pub(crate) fn default_interface_search_path(project: &Path) -> Option<std::ffi::OsString> {
    ws::interface_search_root(project)
        .ok()
        .map(PathBuf::into_os_string)
}

/// The interface search path a configure of `project` runs with, passed as
/// an argument to sync, the stamp and `nros build` (Ruling R45): the user's,
/// else [`default_interface_search_path`]. RFC-0120 D2: configure computes it
/// itself, which retires `eval "$(nros dev ws env)"`. Derived per PROJECT
/// (Ruling R12): a process that configures two projects hands each its own.
pub(crate) fn interface_search_path_for(project: &Path) -> Option<std::ffi::OsString> {
    user_interface_search_path()
        .cloned()
        .or_else(|| default_interface_search_path(project))
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread, time::Duration};

    use super::*;

    /// Hold `project`'s configure lock for the rest of the process, the way a
    /// parent configure holds it while its census spawns `nros build`.
    fn hold_lock_forever(project: &Path) {
        let guard = lock_exclusive_path(&configure_stamp::lock_path(project)).unwrap();
        // Never released: a thread still waiting on it must stay waiting,
        // never go on to configure a scratch dir after the test ends.
        std::mem::forget(guard);
    }

    /// Run `ensure_current_with` on a thread; `Some` if it returned in `wait`.
    fn ensure_current_within(project: &Path, held: &Path, wait: Duration) -> Option<Result<()>> {
        let (tx, rx) = mpsc::channel();
        let (project, held) = (project.to_path_buf(), held.to_path_buf());
        thread::spawn(move || {
            let r = ensure_current_with(&project, &[], false, true, Some(held.as_os_str()));
            let _ = tx.send(r);
        });
        rx.recv_timeout(wait).ok()
    }

    #[test]
    fn implicit_configure_under_a_held_parent_does_not_block() {
        let project = crate::test_support::scratch_dir("configure_held_same");
        hold_lock_forever(&project);
        // A non-canonical spelling of the same project still matches.
        let spelled = project.join(".");
        let r = ensure_current_within(&project, &spelled, Duration::from_secs(2))
            .expect("ensure_current blocked on a lock its own parent configure holds");
        r.expect("returns Ok under a held parent");
    }

    #[test]
    fn a_held_marker_for_another_project_still_configures() {
        let project = crate::test_support::scratch_dir("configure_held_mine");
        let other = crate::test_support::scratch_dir("configure_held_other");
        hold_lock_forever(&project);
        // Another project's marker does not exempt this one: it goes on to
        // take its own lock, which is held, so it waits.
        assert!(
            ensure_current_within(&project, &other, Duration::from_secs(1)).is_none(),
            "a marker naming another project skipped this project's configure"
        );
    }

    #[test]
    fn held_marker_names_one_canonical_project() {
        let a = crate::test_support::scratch_dir("configure_held_a");
        let b = crate::test_support::scratch_dir("configure_held_b");
        assert!(held_by_parent(&a, Some(a.join(".").as_os_str())));
        assert!(!held_by_parent(&a, Some(b.as_os_str())));
        assert!(!held_by_parent(&a, None));
        assert!(!held_by_parent(&a, Some(std::ffi::OsStr::new(""))));
    }

    fn record_ament(project: &Path, key: &str, ament: Option<&str>, resolver: Option<&Path>) {
        let stamp = configure_stamp::Stamp {
            digest: "sha256:0".into(),
            codegen_version: String::new(),
            ament_prefix: ament.map(str::to_string),
            launch_resolve: resolver.map(Path::to_path_buf),
            resolver_pin: Some(configure_stamp::resolver_pin()),
            skipped: None,
        };
        configure_stamp::record(project, key, &stamp).unwrap();
    }

    /// RFC-0120 D2 (Ruling R32): a stale stamp, run with no sourced ROS
    /// shell, reuses the recorded `AMENT_PREFIX_PATH` (and resolver) for C1.
    /// Before this the recorded prefix was written and never read, so C1 ran
    /// with no ament and failed.
    #[test]
    fn stale_configure_reuses_the_recorded_ament_prefix() {
        let p = crate::test_support::scratch_dir("configure-reuse-ament");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        let resolver = p.join("nros-launch-resolve");
        std::fs::write(&resolver, "").unwrap();
        record_ament(&p, "linux", Some("/opt/ros/humble"), Some(&resolver));

        let restore = recorded_env_to_restore(&p, &["linux"], false, false).unwrap();
        assert_eq!(
            restore,
            vec![
                (configure_stamp::AMENT_VAR, "/opt/ros/humble".into()),
                (
                    configure_stamp::LAUNCH_RESOLVE_VAR,
                    resolver.clone().into_os_string()
                ),
            ]
        );
        // A caller whose environment has both keeps its own.
        assert!(
            recorded_env_to_restore(&p, &["linux"], true, true)
                .unwrap()
                .is_empty()
        );
        // A recorded resolver that is gone is not restored.
        std::fs::remove_file(&resolver).unwrap();
        assert_eq!(
            recorded_env_to_restore(&p, &["linux"], false, false).unwrap(),
            vec![(configure_stamp::AMENT_VAR, "/opt/ros/humble".into())]
        );
    }

    /// RFC-0120 D2 (Ruling R32): nothing recorded and nothing in the
    /// environment — refuse, naming the setup step, rather than run C1 with
    /// no ament. A stamp that recorded NO prefix is still a record.
    #[test]
    fn no_record_and_no_env_refuses_naming_the_setup_step() {
        let p = crate::test_support::scratch_dir("configure-no-ament");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        // A package with a message dependency: C1 has messages to generate.
        std::fs::write(p.join("package.xml"), package_xml("talker", &["std_msgs"])).unwrap();
        let err = recorded_env_to_restore(&p, &["linux"], false, true)
            .expect_err("no record and no AMENT_PREFIX_PATH must refuse");
        let msg = format!("{err:#}");
        assert!(msg.contains("AMENT_PREFIX_PATH"), "{msg}");
        assert!(msg.contains("setup.bash"), "{msg}");
        // With the variable set there is nothing to reuse and nothing to refuse.
        assert!(
            recorded_env_to_restore(&p, &["linux"], true, true)
                .unwrap()
                .is_empty()
        );
        // A record with no prefix: nothing to restore, no refusal.
        record_ament(&p, "linux", None, None);
        assert!(
            recorded_env_to_restore(&p, &["linux"], false, true)
                .unwrap()
                .is_empty()
        );
    }

    fn package_xml(name: &str, deps: &[&str]) -> String {
        let deps: String = deps
            .iter()
            .map(|d| format!("<depend>{d}</depend>"))
            .collect();
        format!(
            "<package format=\"3\"><name>{name}</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license>{deps}</package>"
        )
    }

    /// Ruling R44: R32's refusal is for a C1 that NEEDS ament — one with a
    /// message package to generate. A project with no message dependency
    /// configures with no `AMENT_PREFIX_PATH` and no record, on any host.
    #[test]
    fn a_project_with_no_messages_configures_without_ament() {
        let p = crate::test_support::scratch_dir("configure-no-msgs");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("package.xml"), package_xml("plain", &["rclrs"])).unwrap();
        assert!(
            recorded_env_to_restore(&p, &["linux"], false, true)
                .expect("no message to generate, so nothing needs ament")
                .is_empty()
        );
    }

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

    fn explicit_args(argv: &[&str]) -> Args {
        #[derive(Parser)]
        struct A {
            #[command(flatten)]
            args: Args,
        }
        let mut full = vec!["configure"];
        full.extend_from_slice(argv);
        A::try_parse_from(full).unwrap().args
    }

    /// Ruling R25 — an EXPLICIT configure under an inherited marker for the
    /// same project fails, naming the variable; before, it printed a note and
    /// exited 0 having done nothing, so `--check` could pass vacuously.
    #[test]
    fn explicit_configure_refuses_an_inherited_held_marker() {
        let project = crate::test_support::scratch_dir("configure_held_explicit");
        let p = project.to_str().unwrap();
        for argv in [
            &[p][..],
            &[p, "--check"],
            &[p, "--dry-run"],
            &[p, "--force"],
        ] {
            let err = configure_with(explicit_args(argv), Some(project.join(".").as_os_str()))
                .expect_err("an explicit configure under the marker must fail");
            let msg = format!("{err:#}");
            assert!(
                msg.contains(
                    "NROS_CONFIGURE_HELD is internal to nros configure (it marks a configure \
                     already running for this project); unset it"
                ),
                "{argv:?}: {msg}"
            );
        }
    }
}
