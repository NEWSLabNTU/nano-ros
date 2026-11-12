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

    /// Take the entity census first: build the native image generated from
    /// the same launch file, run it, and record what it registers. Every
    /// image this configure covers, or only `BRINGUP:IMAGE`: the image whose
    /// build checks the census, usually a cross image, as in
    /// `nros build BRINGUP:IMAGE`. A plain configure only reports a census
    /// that is missing or stale, because taking one builds and runs a
    /// program.
    // Issue 2394, RFC-0117 amendment "user-tier verbs for 2394".
    #[arg(
        long,
        value_name = "BRINGUP:IMAGE",
        num_args = 0..=1,
        default_missing_value = "",
        conflicts_with_all = ["check", "dry_run"]
    )]
    pub census: Option<String>,

    /// Apply the known, reversible layout migrations first, printing every
    /// file moved. Today that is one: a `system.toml` beside a package's
    /// code moves into a bringup package.
    // Issue 2394, RFC-0117 amendment "user-tier verbs for 2394"; the move is
    // RFC-0121 D8.
    #[arg(long, conflicts_with_all = ["check", "dry_run"])]
    pub migrate: bool,
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
    if args.migrate {
        migrate(&project)?;
    }
    if let Some(want) = &args.census {
        let named: Vec<String> = args.sync.image.iter().cloned().collect();
        take_censuses(&project, want, &named, args.sync.offline)?;
    }
    // `--check` and `--dry-run` keep the meaning they had on `nros sync`: a
    // check, and a print of what would be written. Neither writes, so neither
    // plans nor records a stamp.
    if args.sync.check || args.sync.dry_run {
        ws::run_sync(args.sync)?;
        return Ok(Outcome::default());
    }
    let named: Vec<String> = args.sync.image.iter().cloned().collect();
    configure_project(&project, args.sync, &named, args.force, "configure")
}

/// `nros dev bindings` (issue 2318): C1 alone — message bindings, the cargo
/// patch config, the metadata probe, models and the leaf lock seed — under
/// the project lock C1 shares with a full configure. It skips every image's
/// stages (discover, preflight, census, resolve, generate) and records no
/// stamp, because a stamp would claim images this run never planned; the
/// next `nros build` or `nros configure` still configures each one.
///
/// For tooling that wants the generated code and nothing it would build
/// (`just generate-bindings`): an image's census and a shared host
/// configure each compile a native image.
///
/// # Errors
///
/// What C1 returns, or a held project lock.
pub fn bindings_only(mut sync: ws::SyncArgs) -> Result<Outcome> {
    let project = project_root(sync.workspace.as_deref())?;
    sync.interface_search_path = interface_search_path_for(&project);
    let _lock = lock_exclusive_path(&configure_stamp::lock_path(&project))?;
    let before = ws::sync_runs();
    ws::run_sync(sync)?;
    Ok(Outcome {
        sync_passes: ws::sync_runs() - before,
        ..Outcome::default()
    })
}

/// Configure `project` for `images` (empty: every image the project
/// declares) unless every one is already current. `nros build` and `nros run`
/// call this before they plan (RFC-0120 D3); `dry_run` is theirs, handed to
/// stage 4 unchanged. `verb` is the caller's own (`build`, `run`): a refusal
/// tells the reader to run `nros <verb>` again, the command they typed.
///
/// Returns whether a DRY run found an image a real run would configure
/// (issue 2350): its model may be stale only because the dry run did not
/// refresh it, which the caller's model door is then told
/// ([`crate::model_gate::StaleModel::for_dry_run`]). Always `false` for a
/// real run, which configured.
pub fn ensure_current(
    project: &Path,
    images: &[String],
    dry_run: bool,
    offline: bool,
    verb: &str,
    zephyr_workspace: Option<&Path>,
) -> Result<bool> {
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
    let mut sync = SyncOnly::try_parse_from(&argv)
        .wrap_err("configure: building the sync arguments")?
        .sync;
    // Issue 2467 — the caller's `--zephyr-workspace`. Without it, a configure
    // run by `nros build --zephyr-workspace <ws>` planned every Zephyr image
    // on the default line's store workspace, and on a host that has only the
    // named one it refused with "no Zephyr found" before the build ever ran.
    sync.zephyr_workspace = zephyr_workspace.map(Path::to_path_buf);
    // Ruling R43: a dry run never configures, so it writes nothing (no C1
    // codegen, model, lock seed or stamp). It says what a real run would do,
    // and the caller plans from the project as it stands.
    if dry_run {
        let pending = would_configure(&project, &sync, images)?;
        for (key, why) in &pending {
            eprintln!("nros {verb}: --dry-run: would configure {key}: {why}");
        }
        return Ok(!pending.is_empty());
    }
    configure_project(&project, sync, images, false, verb).map(|_| false)
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

/// Issue 2394 -- `--migrate`: move every `system.toml` that sits beside a
/// package's code into a bringup package, through the one migration
/// (`migrate_bringup`), which prints each file it moves. The candidates are
/// the project itself and every package its workspace discovers, filtered by
/// the predicate configure's own warning uses.
fn migrate(project: &Path) -> Result<()> {
    let movable = migratable(project);
    if movable.is_empty() {
        eprintln!("nros configure --migrate: nothing to migrate");
        return Ok(());
    }
    for dir in movable {
        eprintln!(
            "nros configure --migrate: moving {} into a bringup package",
            dir.join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
                .display()
        );
        crate::cmd::migrate_bringup::run(crate::cmd::migrate_bringup::Args {
            dir,
            dry_run: false,
            fixtures: None,
            fixtures_root: None,
        })?;
    }
    Ok(())
}

/// The directories `--migrate` would move a `system.toml` out of: `project`
/// and every package its workspace discovers, by the predicate configure's own
/// warning uses. `nros doctor` asks this too, and writes nothing.
pub(crate) fn migratable(project: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![project.to_path_buf()];
    if let Ok(ws) = crate::orchestration::workspace::Workspace::discover(project) {
        dirs.extend(ws.packages.into_iter().map(|p| p.root));
    }
    dirs.sort();
    dirs.dedup();
    let refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    ws::system_toml_beside_code(&refs)
}

/// Issue 2394 -- `--census [BRINGUP:IMAGE]`: take the census of `want`, or of
/// every image the configure covers when `want` is empty. A take with nothing
/// to do (a host image, a model with no contract, a fresh census) does
/// nothing; one that cannot run fails the configure, naming the image.
fn take_censuses(project: &Path, want: &str, named: &[String], offline: bool) -> Result<()> {
    if is_leaf(project) {
        let image = if want.is_empty() {
            named.first().map(|n| bare_id(n).to_string())
        } else {
            Some(bare_id(want).to_string())
        };
        return crate::cmd::entity_census::take_leaf_for_configure(project, image.as_deref())
            .wrap_err_with(|| format!("nros configure --census: {}", project.display()));
    }
    let images: Vec<String> = if want.is_empty() {
        targets(project, named)?
            .into_iter()
            .map(|t| t.select)
            .collect()
    } else {
        targets(project, &[want.to_string()])?
            .into_iter()
            .map(|t| t.select)
            .collect()
    };
    for q in images {
        crate::cmd::entity_census::take_for_configure(project, &q, offline)
            .wrap_err_with(|| format!("nros configure --census: `{q}`"))?;
    }
    Ok(())
}

/// The shared body of [`configure`] and [`ensure_current`].
fn configure_project(
    project: &Path,
    mut sync: ws::SyncArgs,
    named: &[String],
    force: bool,
    verb: &str,
) -> Result<Outcome> {
    // Ruling R45: the interface search path is an ARGUMENT, derived once
    // here, and never read from or written to the process environment.
    sync.interface_search_path = interface_search_path_for(project);
    let flags = configure_stamp::OutputFlags::of(&sync);
    // Issue 2467 — captured before `run_sync` consumes `sync`.
    let zephyr_workspace = sync.zephyr_workspace.clone();
    let _lock = lock_exclusive_path(&configure_stamp::lock_path(project))?;

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
        verb,
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
    // Issue 2478 — what C1-C6 read outside the project is recorded from here.
    configure_stamp::begin_external_reads();
    let syncs_before = ws::sync_runs();
    ws::run_sync(sync)?;

    // ---- C2-C6: `nros build` stages 1-4, one image at a time ----------
    let mut out = Outcome::default();
    // phase-530 W3 (RFC-0120 D2) — the shared host-configure coordinate each
    // configured image is planned on, by stamp key.
    let mut coordinates: std::collections::BTreeMap<String, build::SharedConfigure> =
        Default::default();
    // phase-530 W4 (RFC-0120 D2) — where each configured image's answer
    // was resolved, by stamp key: the coordinate's union under a shared host
    // configure (W3), else the image's own resolve.
    let mut resolved: std::collections::BTreeMap<String, PathBuf> = Default::default();
    for t in &targets {
        let mut args = build_args(project, &t.select, &nano_ros_path, false, offline);
        args.zephyr_workspace = zephyr_workspace.clone();
        match build::plan_builds(&args) {
            Ok(plans) => {
                let (coord, dir) = planned_facts(project, &plans);
                if let Some(d) = dir {
                    resolved.insert(t.key.clone(), d);
                }
                if let Some(coord) = coord {
                    coordinates.insert(t.key.clone(), coord);
                }
                out.configured.push(t.key.clone());
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
    {
        let stamp = configure_stamp::stamp_now(project, &flags)?;
        let project_wide = targets
            .is_empty()
            .then(|| configure_stamp::PROJECT_KEY.to_string());
        for key in out.configured.iter().chain(project_wide.iter()) {
            let image_stamp = configure_stamp::Stamp {
                coordinate: coordinates.get(key).map(build::SharedConfigure::id),
                resolved_dir: resolved.get(key).cloned(),
                ..stamp.clone()
            };
            configure_stamp::record(project, key, &image_stamp)?;
        }
        // phase-530 W3 (RFC-0120 D2) — "a shared host configure gets one stamp
        // per coordinate", covering the union its resolve composed over.
        let mut recorded = std::collections::BTreeSet::new();
        for coord in coordinates.values() {
            if !recorded.insert(coord.clone()) {
                continue;
            }
            let dir = build::coordinate_resolve_dir(project, coord);
            let covers = build::read_coordinate_members(&dir)
                .map(|m| m.images)
                .unwrap_or_default();
            configure_stamp::record(
                project,
                &configure_stamp::coordinate_key(&coord.id()),
                &configure_stamp::Stamp {
                    covers,
                    ..stamp.clone()
                },
            )?;
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
/// nothing to reuse, so configure refuses: source ROS 2 and run `nros <verb>`
/// again, `verb` being the command the reader typed (issue 2279). A stamp
/// that recorded no prefix (a run that had none) is a record: nothing to
/// restore, and no refusal.
#[cfg(test)]
pub(crate) fn recorded_env_to_restore(
    project: &Path,
    keys: &[&str],
    ament_set: bool,
    resolver_found: bool,
) -> Result<Vec<(&'static str, std::ffi::OsString)>> {
    recorded_env_to_restore_for(project, keys, ament_set, resolver_found, "configure")
}

/// [`recorded_env_to_restore`] for a refusal reached from `nros <verb>`.
#[cfg(test)]
pub(crate) fn recorded_env_to_restore_for(
    project: &Path,
    keys: &[&str],
    ament_set: bool,
    resolver_found: bool,
    verb: &str,
) -> Result<Vec<(&'static str, std::ffi::OsString)>> {
    recorded_env_to_restore_with(
        project,
        interface_search_path_for(project).as_deref(),
        keys,
        ament_set,
        resolver_found,
        verb,
    )
}

/// [`recorded_env_to_restore`] with the interface search path the run uses.
pub(crate) fn recorded_env_to_restore_with(
    project: &Path,
    search: Option<&std::ffi::OsStr>,
    keys: &[&str],
    ament_set: bool,
    resolver_found: bool,
    verb: &str,
) -> Result<Vec<(&'static str, std::ffi::OsString)>> {
    if ament_set && resolver_found {
        return Ok(Vec::new());
    }
    let Some(stamp) = keys.iter().find_map(|k| configure_stamp::read(project, k)) else {
        // Ruling R44: only a C1 with a message package to generate needs
        // ament; a project with none configures on any host.
        if !ament_set && ws::has_messages_to_generate(project, search) {
            // RFC-0120 D2: no record → refuse. Issue 2279: the remedy is a
            // command to retype, the one the reader ran.
            eyre::bail!(
                "configure: `{}` is unset, and `{}` has no earlier configure that recorded one. \
                 Source ROS 2 in this shell (`source /opt/ros/<distro>/setup.bash`, or your \
                 overlay's `install/setup.bash`) and run `nros {verb}` again; later shells need \
                 not source it.",
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
        // Stage 4 never runs west, but stage 3's preflight and the handoff it
        // plans DO resolve a Zephyr (issue 2467): a caller that has one sets
        // this field on the returned `Args`.
        zephyr_workspace: None,
        board: None,
        all: false,
        dry_run,
        verbose: false,
        explain_sizing: false,
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

/// The shared host configure a plan builds on, as
/// [`build::SharedConfigure`]: its bringup and the coordinate of its
/// `build/<coord>/cmake`.
/// What an image's configure stamp records about its plan: the shared host
/// configure coordinate it is planned on, and where its answer was resolved.
/// Only a coordinate whose union stage 3.5 wrote counts: a stamp naming one
/// with no union would keep the image stale. The configure a shared
/// coordinate runs reads its union, so that is the answer `nros locate
/// configured` names; otherwise the image's own resolve.
fn planned_facts(
    project: &Path,
    plans: &[build::ResolvedBuild],
) -> (Option<build::SharedConfigure>, Option<PathBuf>) {
    let coord = plans.iter().find_map(shared_coordinate).filter(|c| {
        build::read_coordinate_members(&build::coordinate_resolve_dir(project, c)).is_some()
    });
    let dir = match &coord {
        Some(c) => Some(build::coordinate_resolve_dir(project, c)),
        None => plans.iter().find_map(|b| resolve_dir_of(project, b)),
    };
    (coord, dir)
}

/// Issue 2386 — record `key`'s configure stamp from the plan just made for
/// it, with the same facts a configure records at its end
/// ([`planned_facts`]), so the image's own host configure, which locates its
/// answer through the stamp (`nros_configured_locate`), can run NOW.
///
/// For a build that runs inside another image's configure: the census take
/// builds the HOST image a cross image is checked against, and on a
/// never-built workspace that host image has no stamp yet (stamps are written
/// after the configure's whole plan loop), so its cmake configure refused
/// "has not been configured" and the cross image then refused "census
/// missing". The flags are a default configure's, so a later configure with
/// other output flags reconfigures it, as it would any image.
pub(crate) fn record_planned(
    project: &Path,
    key: &str,
    plans: &[build::ResolvedBuild],
) -> Result<()> {
    let mut sync = default_sync_args();
    sync.interface_search_path = interface_search_path_for(project);
    let flags = configure_stamp::OutputFlags::of(&sync);
    let stamp = configure_stamp::stamp_now(project, &flags)?;
    let (coord, resolved_dir) = planned_facts(project, plans);
    if let Some(c) = &coord {
        let covers = build::read_coordinate_members(&build::coordinate_resolve_dir(project, c))
            .map(|m| m.images)
            .unwrap_or_default();
        configure_stamp::record(
            project,
            &configure_stamp::coordinate_key(&c.id()),
            &configure_stamp::Stamp {
                covers,
                ..stamp.clone()
            },
        )?;
    }
    configure_stamp::record(
        project,
        key,
        &configure_stamp::Stamp {
            coordinate: coord.as_ref().map(build::SharedConfigure::id),
            resolved_dir,
            ..stamp
        },
    )
}

fn shared_coordinate(plan: &build::ResolvedBuild) -> Option<build::SharedConfigure> {
    let dir = plan.cmake_build_dir.as_deref()?;
    let coord = Path::new(dir).parent()?.file_name()?.to_string_lossy();
    let (bringup, _) = plan.qualified.split_once(':')?;
    Some(build::SharedConfigure::new(bringup, coord))
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
pub(crate) fn is_leaf(project: &Path) -> bool {
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

    use super::*;

    fn record_ament(project: &Path, key: &str, ament: Option<&str>, resolver: Option<&Path>) {
        let stamp = configure_stamp::Stamp {
            format: configure_stamp::STAMP_FORMAT,
            digest: "sha256:0".into(),
            codegen_version: String::new(),
            ament_prefix: ament.map(str::to_string),
            launch_resolve: resolver.map(Path::to_path_buf),
            resolver_pin: Some(configure_stamp::resolver_pin()),
            cli_planning: Some(configure_stamp::cli_planning()),
            skipped: None,
            coordinate: None,
            covers: Vec::new(),
            resolved_dir: None,
            inputs: Vec::new(),
            censuses: Default::default(),
            external: Default::default(),
        };
        configure_stamp::record(project, key, &stamp).unwrap();
    }

    /// Issue 2386 -- a stamp recorded from a plan, before the configure's own
    /// end-of-run stamps, is a real one: it exists where the host configure
    /// looks (`nros_configured_locate`) and reads current to a later default
    /// configure, so that configure neither fails nor redoes the image.
    #[test]
    fn a_stamp_recorded_from_a_plan_is_current_to_a_default_configure() {
        let p = crate::test_support::scratch_dir("configure-record-planned");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let key = "action_bringup:linux_action_server";
        record_planned(&p, key, &[]).unwrap();
        let mut sync = default_sync_args();
        sync.interface_search_path = interface_search_path_for(&p);
        assert_eq!(
            configure_stamp::is_current(&p, key, false, &configure_stamp::OutputFlags::of(&sync))
                .unwrap(),
            Currency::Current
        );
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
    ///
    /// Issue 2279: the refusal names the remedy as a command to retype, the
    /// verb the reader ran (`nros run` and `nros build` configure implicitly),
    /// and never "the setup step", which reads as `nros setup`.
    #[test]
    fn no_record_and_no_env_refuses_naming_the_setup_step() {
        let p = crate::test_support::scratch_dir("configure-no-ament");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        // A package with a message dependency: C1 has messages to generate.
        std::fs::write(p.join("package.xml"), package_xml("talker", &["std_msgs"])).unwrap();
        for verb in ["run", "build", "configure"] {
            let err = recorded_env_to_restore_for(&p, &["linux"], false, true, verb)
                .expect_err("no record and no AMENT_PREFIX_PATH must refuse");
            let msg = format!("{err:#}");
            assert!(msg.contains("AMENT_PREFIX_PATH"), "{msg}");
            assert!(msg.contains("source /opt/ros/<distro>/setup.bash"), "{msg}");
            assert!(msg.contains(&format!("run `nros {verb}` again")), "{msg}");
            assert!(!msg.contains("setup step"), "{msg}");
            for other in ["run", "build", "configure"].iter().filter(|o| **o != verb) {
                assert!(!msg.contains(&format!("`nros {other}`")), "{verb}: {msg}");
            }
        }
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
}
