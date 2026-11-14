//! phase-307 W2 — the producer trigger.
//!
//! [`metadata_build`](super::metadata_build) can produce a component's
//! `source-metadata.json`, and W1 made every shipping Rust Node pkg a
//! candidate. Neither is worth anything to a bake while the only way to run it
//! is a user typing `nros metadata --build`: a bake cannot depend on an input
//! that may or may not exist and may or may not describe the current source.
//!
//! This module is the ordering guarantee. `nros sync` calls
//! [`refresh_stale_sidecars`] as its last step — after interface codegen and
//! after the `[patch.crates-io]` tables are written, because the harness
//! compiles the Node pkg for real and its generated interface deps resolve
//! only through those patches.
//!
//! **Staleness is content-addressed, not mtime-based.** Every sidecar carries a
//! [`SourceMetadataProvenance`] digest of the sources it was derived from; a
//! refresh recomputes that digest and rebuilds only on a mismatch. mtimes were
//! rejected deliberately: a `git pull`, rebase, or `git stash pop` rewrites
//! tracked files without changing their content, and an mtime-keyed cache
//! reads STALE for the entire tree afterwards — the "fixture mtime treadmill"
//! this repo already pays for elsewhere. A digest is immune, and it also lets
//! a consumer tell a fresh sidecar from museum data without re-running
//! anything.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use eyre::{Result, WrapErr};
use sha2::Digest as _;

use super::{
    metadata_build::{MetadataBuildOptions, build_metadata, host_triple, probe_blocker},
    probe_seeds::{ProbeSeedRecord, SeedModels, Seeding},
    source_metadata::{ComponentLanguage, SourceMetadata, SourceMetadataProvenance},
    workspace::{ComponentDeclaration, Workspace},
};

/// Directory names never hashed as component source: build output and the
/// sidecar dir itself (hashing the output would make the digest self-
/// referential).
use crate::build_output::is_build_output_dir;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RefreshReport {
    /// Sidecars produced or updated.
    pub rebuilt: Vec<PathBuf>,
    /// Sidecars whose digest already matched the sources.
    pub fresh: Vec<PathBuf>,
    /// Components skipped because no producer exists for their language yet
    /// (C/C++ — phase-307 W3). Reported, never silently dropped.
    pub unsupported: Vec<String>,
    /// Issue 2608 — components whose probe BUILD FAILED, now or at this exact
    /// source on an earlier sync (issue 1469's absorbing skip). A failure is
    /// not "no producer": the command reports it apart and exits non-zero
    /// ([`RefreshReport::check_failures`]).
    pub failed: Vec<ProbeFailure>,
    /// Issue 2608 — deploy-bound standalone examples (#0288) whose host probe
    /// failed. Their image is not built for the host, so falling back to the
    /// SystemModel bound is the designed outcome: reported, not fatal.
    pub degraded: Vec<String>,
    /// Issue 1639 — sidecars the probe wrote for a component no declaration
    /// names any more (a rename, a deletion), removed this pass.
    pub pruned: Vec<PathBuf>,
    /// Issue 1639 — `*.json` files in a sidecar directory that name no
    /// declared component and carry no probe provenance, so this pass did not
    /// write them and will not delete them. Every reader that globs the
    /// directory still sees them, so they are REPORTED.
    pub undeclared_kept: Vec<PathBuf>,
    /// Issue 2640 -- components whose launch instances disagree on their
    /// parameters and whose probe ran, and succeeded, on the declaration
    /// defaults (phase-566 D2): one line each, said and not fatal.
    pub notes: Vec<String>,
}

/// Issue 2608 — what a failed probe does to the command's exit status.
///
/// Declared by the caller in `NROS_PROBE_FAILURE`, never inferred from a TTY
/// or `CI`: an interactive `nros build` / `configure` keeps `warn` (a person
/// reads the warning, and the build does run), while every lane where nobody
/// reads it -- the fixture builders, the `just` lanes, the release dry run --
/// declares `refuse` (issue 2520).
static PINNED_PROBE_FAILURE_POLICY: std::sync::OnceLock<ProbeFailurePolicy> =
    std::sync::OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProbeFailurePolicy {
    #[default]
    Warn,
    Refuse,
}

impl ProbeFailurePolicy {
    pub const ENV: &'static str = "NROS_PROBE_FAILURE";

    /// Read [`Self::ENV`]: unset or empty is `warn`; any other value than
    /// `warn` or `refuse` is an error, never a silent default.
    ///
    /// A policy [`Self::pin`]ned for this process wins over the environment.
    pub fn from_env() -> Result<Self> {
        if let Some(pinned) = PINNED_PROBE_FAILURE_POLICY.get() {
            return Ok(*pinned);
        }
        Self::parse(std::env::var(Self::ENV).ok().as_deref())
    }

    /// State the policy for the rest of this process, over whatever
    /// [`Self::ENV`] says. For an in-process caller whose subject is not the
    /// policy: a test binary must not change its verdict with the
    /// environment it was started from (the `just` lanes export `refuse`, a
    /// bare `cargo test` does not), and setting the variable instead would
    /// race every thread that reads the environment. The first pin wins; a
    /// later pin of a different policy is an error, never a silent switch.
    pub fn pin(policy: Self) -> Result<()> {
        let held = *PINNED_PROBE_FAILURE_POLICY.get_or_init(|| policy);
        if held != policy {
            eyre::bail!(
                "{} is already pinned to {held:?} in this process",
                Self::ENV
            );
        }
        Ok(())
    }

    fn parse(value: Option<&str>) -> Result<Self> {
        match value.map(str::trim) {
            None | Some("") | Some("warn") => Ok(Self::Warn),
            Some("refuse") => Ok(Self::Refuse),
            Some(other) => eyre::bail!(
                "{}={other:?} is not a policy; use `warn` or `refuse`",
                Self::ENV
            ),
        }
    }
}

/// Issue 2608 — one failed probe: which node, the first error line, and where
/// the whole error is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeFailure {
    /// `<package>::<component>`.
    pub node: String,
    /// The first line that reads as an error (see [`first_error_line`]).
    pub first_error: String,
    /// Where the full text is: the component's failure marker, when one was
    /// written.
    pub log: Option<PathBuf>,
}

impl ProbeFailure {
    fn new(node: String, text: &str, log: Option<PathBuf>) -> Self {
        Self {
            node,
            first_error: first_error_line(text),
            log,
        }
    }
}

impl std::fmt::Display for ProbeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.node, self.first_error)?;
        if let Some(log) = &self.log {
            write!(f, " (full error: {})", log.display())?;
        }
        Ok(())
    }
}

impl RefreshReport {
    pub fn total(&self) -> usize {
        self.rebuilt.len() + self.fresh.len()
    }

    /// Issue 2608 — the exit status a command reports for this refresh, under
    /// `policy`. A component with no producer, or a deploy-bound degrade, is
    /// never an error. A failed probe is an error under
    /// [`ProbeFailurePolicy::Refuse`], and under `Warn` when NO probe of the
    /// run succeeded (nothing was rebuilt or current), because then every
    /// component is on default sizes.
    pub fn check_failures(&self, policy: ProbeFailurePolicy) -> Result<()> {
        if self.failed.is_empty() {
            return Ok(());
        }
        let all_failed = self.total() == 0;
        if policy == ProbeFailurePolicy::Warn && !all_failed {
            return Ok(());
        }
        let mut msg = format!(
            "{} metadata probe(s) failed, so their components have no source metadata \
             and would be built on default sizes:",
            self.failed.len()
        );
        for f in &self.failed {
            msg.push_str(&format!("\n  {f}"));
        }
        msg.push_str(&format!(
            "\nNote: {}.",
            crate::orchestration::metadata_probe_cmake::PROBE_ARGS_RULE
        ));
        if policy == ProbeFailurePolicy::Warn {
            msg.push_str(
                "\nNo probe in this workspace succeeded, so this is refused even under \
                          NROS_PROBE_FAILURE=warn.",
            );
        }
        eyre::bail!(msg)
    }

    /// Issue 2608 — the closing summary a command prints for this refresh, or
    /// `None` when no probe failed. It is the LAST line, so the count cannot
    /// scroll away behind a long build.
    pub fn failure_summary(&self) -> Option<String> {
        (!self.failed.is_empty()).then(|| {
            format!(
                "{} metadata probe(s) FAILED (listed above); their components keep default \
                 sizes. NROS_PROBE_FAILURE=refuse makes this an error.",
                self.failed.len()
            )
        })
    }
}

/// Refresh every stale source-metadata sidecar in the workspace.
///
/// `nano_ros` is the nano-ros checkout the harness path-depends on for `nros`;
/// without it no harness can be compiled and the whole step is skipped (a
/// user's workspace built against an installed SDK has no such path).
///
/// `model_dir` is the models directory the sync wrote into when it was told one
/// (`--model-dir`); `None` is `NROS_MODEL_DIR`, else `<ws>/build/nros/models`.
/// The C/C++ probes are seeded from the models there (issue 2640).
pub fn refresh_stale_sidecars(
    ws_root: &Path,
    nano_ros: Option<&Path>,
    verbose: bool,
    model_dir: Option<&Path>,
) -> Result<RefreshReport> {
    let mut report = RefreshReport::default();
    let workspace = Workspace::discover(ws_root)?;
    let declarations = workspace.component_declarations()?;
    // Issue 2721 -- what the probe build must hold a table for, computed once.
    let tables_needed = probe_tables_needed(ws_root, &declarations);
    // Issue 2425 — a bringup row whose workspace package no declaration
    // covers is a component this pass cannot probe. Said, never dropped: the
    // descriptor's answer for it is a refusal, which is safe and therefore
    // otherwise invisible.
    for (system_toml, pkg) in workspace.undeclared_component_rows()? {
        report.unsupported.push(format!(
            "{pkg} (named by `[[component]] pkg` in {}, but nothing in the package \
             declares a node -- its subscriptions are sized as buffered; give it a \
             `[package.metadata.nros.node]` table, or a `nano_ros_node_register` call \
             for a CMake package)",
            system_toml.display()
        ));
    }
    if declarations.is_empty() {
        return Ok(report);
    }
    // Issue 1639 — before anything is probed, so a stale sidecar can never be
    // read by this pass either.
    prune_undeclared_sidecars(&declarations, &mut report);
    let probe_root = ws_root.join("build").join("nros-metadata");
    // RFC-0117 D8: the probe's arguments are the workspace-wide block's, set
    // before any freshness check so a change re-probes.
    crate::orchestration::metadata_probe_cmake::set_probe_cmake_args(
        &crate::orchestration::metadata_probe_cmake::probe_dir_for_workspace(&probe_root),
        &workspace.probe_cmake_args()?,
    )?;
    // Issue 0286 — resolved once, not per component.
    let host = host_triple();
    // Issue 2640 (phase-566 D1) -- the launch values of every component, read
    // once from the models the sync just resolved.
    let seed_models = SeedModels::load(ws_root, model_dir);
    // What each C/C++ probe of this pass is seeded with, by
    // `(package, component)`: its key, its sidecar's `probe` object and, when
    // its instances disagree, its note or refusal all come from it.
    let mut cpp_seeding: std::collections::BTreeMap<(String, String), Seeding> =
        std::collections::BTreeMap::new();
    // phase-313 — C/C++ components to probe in ONE workspace-scoped project.
    // Issue 2238 -- every C/C++ component the probe project ADDS (fresh or
    // not), beside the stale ones it re-probes.
    let mut cpp_members: Vec<crate::orchestration::metadata_probe_cmake::CmakeProbeOptions> =
        Vec::new();
    let mut cpp_batch: Vec<crate::orchestration::metadata_probe_cmake::CmakeProbeOptions> =
        Vec::new();

    for decl in &declarations {
        // #0288 — deploy-bound standalone examples (issue 0100: node + deploy
        // target in one crate) are no longer SKIPPED. Their board crates now
        // host-build (the layer-2 `skip_cross_build` gates), so the host probe
        // yields an EXACT executor size instead of the SystemModel's timer-blind
        // lower bound — the sizing SSoT this issue was about. The ones that
        // still cannot host-build are caught by `probe_blocker` just below;
        // anything that fails the probe itself degrades best-effort (Rust branch
        // further down) with a negative-cache marker so it is not rebuilt every
        // sync. `decl.deploy_bound` only selects that best-effort branch — a
        // REGULAR workspace package that fails to probe is a bug and still
        // hard-fails.
        if let Some(blocker) = probe_blocker(&decl.package_root, Some(host.as_str())) {
            // Issue 0286 — the probe cannot produce a runnable binary for this
            // component. Degrade to the sidecar-less bake (SystemModel bound)
            // instead of failing the whole build, which is what used to take
            // the nuttx fixture lane — and therefore `just ci` — down.
            report.unsupported.push(format!(
                "{}::{} ({blocker})",
                decl.config.package, decl.config.component
            ));
            continue;
        }
        if decl.config.language != ComponentLanguage::Rust {
            // phase-313 — C/C++ probes are COLLECTED here and run as one
            // batch below, so the workspace builds the runtime (and the ~1.2 GB
            // of nested sizes-probe cargo builds) once rather than once per
            // component.
            let seeding = seeding_of(&seed_models, decl);
            let node = (decl.config.package.clone(), decl.config.component.clone());
            cpp_seeding.insert(node, seeding.clone());
            match cpp_probe_options(
                decl,
                nano_ros,
                &probe_root,
                &seeding,
                tables_needed.as_deref(),
            ) {
                // issue 0641 — a C/C++ probe that FAILED at this exact source is
                // not re-attempted until something changes. The Rust branch
                // below has had this since #0288; the batch path never did, so
                // an unprobeable component paid a full CMake configure + build
                // on EVERY sync, for ever. Measured on
                // `examples/workspaces/features`, whose probe project fails to
                // configure: 17 components, 1.2 s of cmake per sync, no
                // possible progress. `nros sync` is run 22 times by
                // `regenerate-bindings.sh` at the head of every fixture build.
                Ok(Some(opts))
                    if is_known_unprobeable_now(&opts.output_path, decl, nano_ros, &seeding) =>
                {
                    // issue 1469 — carry the RECORDED CAUSE up, not just the
                    // fact of the skip. "probe failed at this source last sync;
                    // unchanged" named the node and nothing else, and because
                    // the skip never re-attempts, the error it stands for is
                    // reachable from no later sync at all. The marker holds the
                    // first error line; say it here, and where the whole file
                    // is, so the next reader starts at the cause.
                    // Issue 2608 — a recorded failure is a failure.
                    let why = recorded_unprobeable_reason(&opts.output_path).unwrap_or_else(|| {
                        "probe failed at this source on an earlier sync".to_string()
                    });
                    report.failed.push(ProbeFailure::new(
                        format!("{}::{}", decl.config.package, decl.config.component),
                        &format!("{why} (recorded on an earlier sync; the source is unchanged)"),
                        Some(unprobeable_marker(&opts.output_path)),
                    ));
                    if verbose {
                        println!(
                            "sync: metadata {}::{} — skipped; recorded failure in {}",
                            decl.config.package,
                            decl.config.component,
                            unprobeable_marker(&opts.output_path).display()
                        );
                    }
                }
                Ok(Some(opts)) => {
                    cpp_members.push(opts.clone());
                    cpp_batch.push(opts);
                }
                // Already current — no probe needed, but still a MEMBER of the
                // probe project (issue 2238): its configure writes the
                // aggregate every member's row must stay in.
                Ok(None) => {
                    if let Ok(opts) = cpp_probe_member(decl, nano_ros, &probe_root, &seeding) {
                        cpp_members.push(opts);
                    }
                    report.fresh.push(decl.source_metadata_path())
                }
                Err(why) => report.unsupported.push(format!(
                    "{}::{} ({why})",
                    decl.config.package, decl.config.component
                )),
            }
            continue;
        }
        let sidecar = decl.source_metadata_path();
        // Issue 1578 — ONE key for every freshness decision below: the
        // component's sources, the CLI, and the nano-ros crates the probe
        // compiles. `None` (no nano-ros path, or no closure list) is never
        // fresh, so the probe re-runs rather than trusting a smaller key.
        let key = probe_inputs_key(&decl.package_root, nano_ros);
        if key
            .as_deref()
            .is_some_and(|k| sidecar_is_fresh(&sidecar, k))
        {
            clear_unprobeable(&sidecar);
            report.fresh.push(sidecar);
            continue;
        }
        // #0288 negative cache — a deploy-bound example whose probe failed at
        // THIS source last sync is not re-attempted (a full failing build) until
        // its sources change and the digest differs.
        if decl.deploy_bound
            && key
                .as_deref()
                .is_some_and(|k| is_known_unprobeable(&sidecar, k, nano_ros))
        {
            // issue 1469, same class as the C/C++ branch above: the skip is
            // absorbing, so the marker's recorded cause is the only place the
            // reason survives.
            let why = match recorded_unprobeable_reason(&sidecar) {
                Some(line) => format!("; last error: {line}"),
                None => String::new(),
            };
            report.degraded.push(format!(
                "{}::{} (deploy-bound: probe failed at this source last sync; unchanged{why})",
                decl.config.package, decl.config.component
            ));
            if verbose {
                println!(
                    "sync: metadata {}::{} — skipped; recorded failure in {}",
                    decl.config.package,
                    decl.config.component,
                    unprobeable_marker(&sidecar).display()
                );
            }
            continue;
        }
        let Some(nano_ros) = nano_ros else {
            // No harness is buildable. Not an error — a sidecar-less bake
            // falls back to the SystemModel bound — but never silently
            // pretend the sidecar is current.
            report.unsupported.push(format!(
                "{}::{} (no nano-ros path)",
                decl.config.package, decl.config.component
            ));
            continue;
        };
        if verbose {
            println!(
                "sync: metadata {}::{} — sources changed, rebuilding",
                decl.config.package, decl.config.component
            );
        }
        let opts = build_options(decl, nano_ros, &probe_root);
        match build_metadata(&opts) {
            Ok(()) => {
                clear_unprobeable(&sidecar);
                stamp_provenance(&sidecar, key.as_deref().unwrap_or(UNKEYED))?;
                report.rebuilt.push(sidecar);
            }
            // #0288 — a deploy-bound standalone example degrades best-effort:
            // record the failure (negative cache) and fall back to the
            // SystemModel bound, exactly as before the flip, instead of failing
            // the whole sync. A REGULAR workspace package that fails to probe is
            // a real bug and still hard-fails.
            //
            // Issue 1817 — so is a deploy-bound package whose OWN manifest does
            // not resolve: that fails on every target, not just the host, so
            // the degrade would only move the error from `nros sync` (which
            // printed `done.` and exited 0) to the build. No marker either: a
            // resolution failure costs no compile to re-check.
            Err(why) if decl.deploy_bound => {
                if let Some(cause) =
                    crate::orchestration::metadata_build::component_resolution_error(&opts)
                {
                    eyre::bail!(
                        // issue 1817.
                        "{}::{}: cargo cannot resolve {}:\n{cause}\n\n\
                         This fails the image's own build too, so sync stops here rather \
                         than fall back to the SystemModel bound.",
                        decl.config.package,
                        decl.config.component,
                        decl.package_root.join("Cargo.toml").display()
                    );
                }
                if let Some(k) = key.as_deref() {
                    // A cargo probe path-depends on `nano_ros` itself, so that
                    // IS the compile root.
                    mark_unprobeable_with_reason(
                        &sidecar,
                        k,
                        Some(nano_ros),
                        Some(&format!("{why:#}")),
                    );
                }
                report.degraded.push(format!(
                    "{}::{} (deploy-bound probe failed: {why:#})",
                    decl.config.package, decl.config.component
                ));
            }
            Err(why) => {
                return Err(why).wrap_err_with(|| {
                    format!("refresh source metadata for `{}`", decl.config.package)
                });
            }
        }
    }

    // phase-313 — one project, one configure, one runtime build; the BUILD is
    // still per target so an uncompilable component costs only its own sidecar.
    if !cpp_batch.is_empty() {
        let dir = crate::orchestration::metadata_probe_cmake::probe_dir_for_workspace(&probe_root);
        if verbose {
            println!(
                "sync: metadata — probing {} C/C++ component(s) in one project",
                cpp_batch.len()
            );
        }
        // Issue 1593 — what the probe build dir ACTUALLY compiled, read back
        // from its cache after the run: the marker records it, so a marker
        // whose compile was another tree reads as stale, never as authority.
        let compile_root =
            || crate::orchestration::metadata_probe_cmake::cached_probe_root(&dir.join("build"));
        let rmws = workspace_image_rmws(&workspace);
        let rmw =
            crate::orchestration::metadata_probe_cmake::probe_rmw(rmws.iter().map(String::as_str));
        match crate::orchestration::metadata_probe_cmake::run_probes(
            &dir,
            &cpp_members,
            &cpp_batch,
            rmw,
        ) {
            Ok(outcomes) => {
                // Matched by name, not zipped: a component dropped to let the
                // rest configure is reported AFTER the ones that ran (phase-367
                // W4), so the outcome order is not the batch order.
                for o in &outcomes {
                    let Some(opts) = cpp_batch
                        .iter()
                        .find(|b| b.package == o.package && b.component == o.component)
                    else {
                        continue;
                    };
                    let node = format!("{}::{}", o.package, o.component);
                    let seeding = cpp_seeding
                        .get(&(o.package.clone(), o.component.clone()))
                        .cloned()
                        .unwrap_or(Seeding::None);
                    // Issue 2640 -- the key the probe ran under, seeds included.
                    let key = component_probe_key(
                        &opts.package_dir,
                        Some(&opts.nano_ros_workspace),
                        &seeding,
                    );
                    match &o.result {
                        Ok(()) => {
                            // Stamp only on success; a failed probe leaves the
                            // previous sidecar (or none) untouched.
                            // Issue 2689 -- NOT `let _ =`. The stamp's parse
                            // is the schema gate on what the probe wrote; a
                            // swallowed failure left the sidecar unstamped, so
                            // it read as stale on every configure and nothing
                            // said why. Fail like the Rust road above does.
                            stamp_probe(
                                &opts.output_path,
                                key.as_deref().unwrap_or(UNKEYED),
                                seeding.record(),
                            )
                            .wrap_err_with(|| {
                                format!(
                                    "stamp the C/C++ probe sidecar for `{}::{}`",
                                    o.package, o.component
                                )
                            })?;
                            if let Some(note) = seeding.unseeded_note(&node) {
                                report.notes.push(note);
                            }
                            report.rebuilt.push(opts.output_path.clone());
                        }
                        Err(why) => {
                            let raw = format!("{why:#}");
                            // Issue 2640 (phase-566 D2) -- an unseeded probe of
                            // instances that disagree fails naming both causes.
                            let text = seeding
                                .refusal(&node, &first_error_line(&raw))
                                .map_or(raw.clone(), |r| format!("{r}\n{raw}"));
                            mark_unprobeable_now(
                                &opts.output_path,
                                key.as_deref(),
                                nano_ros,
                                compile_root().as_deref(),
                                &text,
                            );
                            report.failed.push(ProbeFailure::new(
                                node.clone(),
                                &text,
                                Some(unprobeable_marker(&opts.output_path)),
                            ));
                        }
                    }
                }
            }
            // Issue 1593 — the build dir compiles another tree. That says
            // nothing about any component, so NO marker: report it, naming
            // both roots, and let the next sync try again.
            Err(why)
                if why
                    .downcast_ref::<crate::orchestration::metadata_probe_cmake::ProbeRootMismatch>()
                    .is_some() =>
            {
                for opts in &cpp_batch {
                    report.failed.push(ProbeFailure::new(
                        format!("{}::{}", opts.package, opts.component),
                        &format!("{why}"),
                        None,
                    ));
                }
            }
            // A configure failure is the whole project, not one component, and
            // nothing a component's inputs key can fix: it was a compiler the
            // probe inherited, a missing tool, a broken cache. A marker keyed
            // on the components' sources outlived exactly such a fix and kept
            // every C/C++ component of five workspaces on DEFAULT sizes with
            // one quiet line each. So no marker, and a warning that says the
            // sizes are defaults until the probe configures.
            Err(why) => {
                // Issue 2608 -- every batched component's probe failed, so each
                // is a FAILURE. Still no marker (above): the next sync retries.
                let text = format!(
                    "probe project configure failed (nothing recorded; the next sync \
                     tries again): {why:#}"
                );
                for opts in &cpp_batch {
                    report.failed.push(ProbeFailure::new(
                        format!("{}::{}", opts.package, opts.component),
                        &text,
                        None,
                    ));
                }
            }
        }
    }
    Ok(report)
}

/// phase-313 — build the probe options for one C/C++ component, or explain why
/// it cannot be probed.
///
/// `Ok(None)` = the sidecar is already current. `Err` = this component only;
/// the caller records it in the unsupported ledger and the bake falls back to
/// the SystemModel bound, which is what these components get today.
fn cpp_probe_options(
    decl: &ComponentDeclaration,
    nano_ros: Option<&Path>,
    probe_root: &Path,
    seeding: &Seeding,
    tables_needed: Option<&[String]>,
) -> Result<Option<crate::orchestration::metadata_probe_cmake::CmakeProbeOptions>, String> {
    // The declaration's refusals first, as before the split (issue 2238).
    let opts = cpp_probe_member(decl, nano_ros, probe_root, seeding)?;
    // Issue 1578 — the same key as the Rust branch; `nano_ros` is known
    // present here (`cpp_probe_member` refused otherwise), so a `None` means
    // only that the closure list is missing, which is never fresh.
    // Issue 2640 -- and the launch values it is seeded with.
    if component_probe_key(&decl.package_root, nano_ros, seeding)
        .is_some_and(|k| sidecar_is_fresh(&opts.output_path, &k))
        && probe_build_present(probe_root, tables_needed)
    {
        return Ok(None);
    }
    Ok(Some(opts))
}

/// Issue 2435 -- the probe's OUTPUTS are two: the sidecar beside the package
/// (`metadata/<component>.json`, in the source tree) and the probe project's
/// build, whose `nros-codegen/` holds the bound tables the message-bound seeds
/// read. A sidecar keyed on its inputs survives `rm -rf build`; the tables do
/// not. Judging freshness by the sidecar alone left a clean build dir with no
/// tables, so no message-bound seed was written on any C/C++ west road and the
/// promise check refused. The probe build is one per workspace, so its
/// absence makes every C/C++ component stale; its CMake cache marks a project
/// that was configured here.
///
/// Issue 2721 -- and its OUTPUTS, not a proxy for them. The CMake cache said
/// only that a configure had started here: a probe whose build failed, or
/// whose `nros-codegen/` was removed, left the cache behind, so the refresh
/// called every committed sidecar current, never re-ran the probe, and every
/// C/C++ west seed then found no bound table (measured: a forced configure
/// of `examples/workspaces/cpp` with the tables removed stayed without them
/// and refused 20 seeds). So the build must hold what the readers read: the
/// probe aggregate `nros-metadata.json`, and a bound table for every package
/// in the members' interface closure (`tables_needed`, the closure the seed
/// asks for). `None` -- the closure could not be resolved -- is never
/// present: the probe re-runs rather than trusting a smaller check.
fn probe_build_present(probe_root: &Path, tables_needed: Option<&[String]>) -> bool {
    let dir = crate::orchestration::metadata_probe_cmake::probe_dir_for_workspace(probe_root);
    let build = dir.join("build");
    let Some(needed) = tables_needed else {
        return false;
    };
    let codegen = build.join("nros-codegen");
    build.join("CMakeCache.txt").is_file()
        && crate::orchestration::metadata_probe_cmake::probe_args_current(&dir)
        && build.join("nros-metadata.json").is_file()
        && needed.iter().all(|pkg| {
            ["c", "cpp"].iter().any(|lang| {
                codegen
                    .join(format!("nano_ros_{lang}"))
                    .join(pkg)
                    .join(rosidl_codegen::INVENTORY_JSON_NAME)
                    .is_file()
            })
        })
}

/// Issue 2721 -- the interface packages the probe must hold a bound table for:
/// the closure over the workspace's C/C++ components, by the seed's own
/// function. `None` when it cannot be resolved.
fn probe_tables_needed(
    ws_root: &Path,
    declarations: &[ComponentDeclaration],
) -> Option<Vec<String>> {
    let dirs: Vec<PathBuf> = declarations
        .iter()
        .filter(|d| d.config.language != ComponentLanguage::Rust)
        .map(|d| d.package_root.clone())
        .collect();
    let search = crate::cmd::configure::interface_search_path_for(ws_root);
    crate::message_bound_knobs::interface_closure(&dirs, search.as_deref().and_then(|s| s.to_str()))
        .ok()
}

/// Issue 2238 -- the probe options of a C/C++ component whatever its
/// freshness: a fresh one is still a member of the probe project.
fn cpp_probe_member(
    decl: &ComponentDeclaration,
    nano_ros: Option<&Path>,
    probe_root: &Path,
    seeding: &Seeding,
) -> Result<crate::orchestration::metadata_probe_cmake::CmakeProbeOptions, String> {
    // Issue 2280 -- a package whose CMakeLists.txt calls
    // `find_package(Zephyr)` is a west application, and the probe adds it to a
    // HOST CMake project, where Zephyr's boilerplate refuses with "BOARD is not
    // being defined" (or, without ZEPHYR_BASE, cannot find Zephyr at all). That
    // failure printed a CMake error into every first `nros configure` of such a
    // leaf, poisoned the configure of every other component in the batch, and
    // ended in the same `unprobeable` answer this returns without running
    // cmake. RFC-0120 names the census, not the probe, as what sizes it.
    if crate::builder::west_app::cmakelists_finds_zephyr(&decl.package_root) {
        return Err(
            "a west application (its CMakeLists.txt calls find_package(Zephyr)), which a host \
             CMake probe cannot configure; such a leaf is sized from its census or its \
             declared entities instead"
                .into(),
        );
    }
    let Some(nano_ros) = nano_ros else {
        return Err("no nano-ros path".into());
    };
    let Some(class) = decl.class.clone() else {
        return Err("no declared CLASS — nothing to construct".into());
    };
    let Some(header) = decl.probe_header() else {
        return Err("no declared HEADER and none derivable from CLASS".into());
    };
    let Some(library_target) = decl.library_target.clone() else {
        return Err("no CMake library target to link".into());
    };
    let sidecar = decl.source_metadata_path();
    // Issue 2680 -- the workspace packages the probe adds before this one. A
    // cycle, or a dependency the host probe cannot configure, refuses the
    // component, naming why.
    let deps = crate::orchestration::probe_deps::workspace_deps(&decl.package_root)?;

    // Issue 1528 — the C-vs-C++ decision, with NO wildcard.
    //
    // This read `C => "c", _ => "cpp"`. It was CORRECT, and only because of a
    // predicate in another function: the one call site above is inside
    // `if decl.config.language != ComponentLanguage::Rust`, so `Rust` never
    // arrived (measured: 79 Rust declarations in the tree, 0 of them here).
    // But the guard and this decision are one fact spelled in two places, and
    // a wildcard is exactly the construct that keeps the compiler from
    // relating them — a fourth `Language` variant passes the `!= Rust` guard
    // and would have been called C++ here in silence. Issue 1062 is what that
    // lands as: an undefined reference against the other ABI seam, two layers
    // down in generated code, surfacing as one `no producer for <pkg>::<comp>`
    // line. So `Rust` is REFUSED rather than absorbed; the caller records the
    // reason in `RefreshReport::unsupported` and `nros sync` prints it, which
    // is the "a probe outcome carries its cause" property issue 1469 landed.
    //
    // The match itself lives in `metadata_probe_cmake::probe_language`, the
    // module that knows what the probe can serve; the emitter asks the same
    // function, so the refusal has one spelling (phase-469).
    let language = crate::orchestration::metadata_probe_cmake::probe_language(decl.config.language)
        .map_err(|why| {
            format!(
                "{why} — reaching the probe is a routing bug, since the caller's \
                     `language != Rust` guard should have sent it to `build_metadata`"
            )
        })?;
    Ok(
        crate::orchestration::metadata_probe_cmake::CmakeProbeOptions {
            package: decl.config.package.clone(),
            component: decl.config.component.clone(),
            executable: decl
                .config
                .linkage
                .resolved_executable(&decl.config.component),
            class,
            header,
            language,
            shape: decl.probe_shape().to_string(),
            library_target,
            package_dir: decl.package_root.clone(),
            nano_ros_workspace: nano_ros.to_path_buf(),
            output_path: sidecar,
            probe_dir: probe_root.to_path_buf(),
            deps,
            seeds: seeding.params().to_vec(),
        },
    )
}

/// Every image's resolved RMW across the workspace's `system.toml` files (a
/// file with no `[image.*]` contributes its system default). A file that does
/// not parse contributes nothing here; the build reports it where it is read.
fn workspace_image_rmws(workspace: &Workspace) -> Vec<String> {
    use crate::orchestration::cargo_metadata_schema::SystemToml;
    let mut out = Vec::new();
    for path in workspace
        .packages
        .iter()
        .filter_map(|p| p.system_toml.as_ref())
    {
        let Some(sys) = nros_system_include::read_to_string(path)
            .ok()
            .and_then(|raw| toml::from_str::<SystemToml>(&raw).ok())
        else {
            continue;
        };
        if sys.image.is_empty() {
            out.push(sys.resolved_rmw(None, None));
        }
        for id in sys.image.keys() {
            out.push(sys.resolved_rmw(Some(id), None));
        }
    }
    out
}

fn build_options(
    decl: &ComponentDeclaration,
    nano_ros: &Path,
    probe_root: &Path,
) -> MetadataBuildOptions {
    let id = decl.config.component.clone();
    let name = id.rsplit("::").next().unwrap_or(&id).to_string();
    MetadataBuildOptions {
        component_id: id.clone(),
        class: decl.class.clone(),
        crate_name: decl.crate_name.clone(),
        package: decl.config.package.clone(),
        executable: Some(decl.config.linkage.resolved_executable(&name)),
        exported_symbol: Some(decl.config.linkage.resolved_exported_symbol(&name)),
        component: name,
        component_dir: decl.package_root.clone(),
        nano_ros_workspace: nano_ros.to_path_buf(),
        output_path: decl.source_metadata_path(),
        harness_dir: probe_root
            .join("metadata-probe")
            .join(id.replace("::", "__")),
    }
}

/// Issue 1594 — the workspace's CURRENT probe sidecars, as one inventory, for a
/// consumer that joins their per-endpoint observations onto a model-road row.
///
/// Only FRESH sidecars count — the key [`refresh_stale_sidecars`] itself trusts
/// ([`probe_inputs_key`]). A stale one describes source that may since have
/// moved to another subscription entry point, and an `in_place: true` read off
/// it would price that endpoint at no receive region (issue 1578 measured a
/// stale `in_place` being reused). A stale, missing or unreadable sidecar is
/// SKIPPED with a note, which leaves its rows unobserved: the refusal, never
/// the under-size.
///
/// Two sidecars naming one component are both dropped: the join keys rows by
/// component, so either would be attributed to the other's endpoints.
///
/// Returns the inventory, the sidecar paths it CONSULTED (configure-time inputs —
/// issue 1018; stale ones included, issue 2361), and one note per skip.
pub fn fresh_probe_inventory(
    ws_root: &Path,
    nano_ros: Option<&Path>,
) -> Result<(
    crate::entity_inventory::EntityInventory,
    Vec<PathBuf>,
    Vec<String>,
)> {
    use crate::entity_inventory::EntityInventory;

    let (rows, read, mut notes) = fresh_probe_rows(ws_root, nano_ros)?;
    let mut inv = EntityInventory::new("source-metadata");
    for row in &rows {
        if rows
            .iter()
            .filter(|r| r.entities.component == row.entities.component)
            .count()
            > 1
        {
            notes.push(format!(
                "component `{}` has more than one probe sidecar in this workspace, so neither's \
                 observations are attributed",
                row.entities.component
            ));
            continue;
        }
        inv.insert(row.entities.clone());
    }
    notes.dedup();
    Ok((inv, read, notes))
}

/// Issue 1694 -- compose a model-road inventory with what the CODE of every
/// launched node was recorded to create, so a contract can refine the counts
/// of the endpoints it names and never shrink one below the code's.
///
/// `contract` is [`crate::entity_inventory::EntityInventory::from_model`]'s
/// answer for `model`. The result is
/// `recorded_for_launch(model, sidecars).merged_per_kind_max(contract)` --
/// the declaration on the left and the contract on the right, the direction
/// and the function the cmake road composes `nros-metadata.json` with -- so
/// every road that feeds a count from a contract applies ONE rule.
///
/// Never fatal: a workspace that does not discover leaves every launched node
/// `Absent`, which lets a contract that describes the node stand and refuses a
/// derivation for one it does not (the crate defaults, never a zero).
///
/// Returns the composed inventory, the sidecars READ (configure inputs, issue
/// 1018), and notes for the caller to print.
pub fn floor_by_recorded(
    ws_root: &Path,
    model: &ros_launch_manifest_model::SystemModel,
    contract: &crate::entity_inventory::EntityInventory,
    // Issue 2734 -- and the stale sidecars it skipped, for the verdict.
) -> (
    crate::entity_inventory::EntityInventory,
    Vec<PathBuf>,
    Vec<String>,
    Vec<StaleSidecar>,
) {
    let nano_ros = crate::orchestration::nano_ros_root::resolve(None, ws_root);
    let (rows, read, mut notes, stale) = match probe_rows_and_stale(ws_root, nano_ros.as_deref()) {
        Ok(v) => v,
        Err(e) => (
            Vec::new(),
            Vec::new(),
            vec![format!(
                "no probe sidecar read ({e}); every launched node the contract does not \
                     describe refuses the derivation"
            )],
            Vec::new(),
        ),
    };
    let recorded = crate::entity_inventory::EntityInventory::recorded_for_launch(model, &rows);
    notes.dedup();
    (recorded.merged_per_kind_max(contract), read, notes, stale)
}

/// Issue 1694 -- THE contract inventory every model road composes: what
/// [`crate::entity_inventory::EntityInventory::from_model`] makes of `model`,
/// floored by [`floor_by_recorded`] when the road knows its workspace.
///
/// With no contract authored (`from_model` is `None`), the image's FRESH probe
/// sidecars stand as its declaration when they describe every launched node
/// ([`probe_inventory`], phase-517 W5); otherwise `None`, so the "no contract,
/// no change" control every caller guards on holds for every image the probe
/// does not fully describe. `who` prefixes the notes this prints.
///
/// `Err` when a package the model launches has a STALE probe sidecar: its code
/// was described and the description could not be read, so the counts are
/// declared and not derived. Each caller reports it under
/// `NROS_PROBE_FAILURE`, never as "nothing declared" (issue 2734).
pub fn contract_inventory(
    source: impl Into<String>,
    model: &nros_orchestration_ir::overlay::LoadedModel,
    workspace: Option<&Path>,
    who: &str,
) -> std::result::Result<Option<crate::entity_inventory::EntityInventory>, String> {
    let source = source.into();
    // Issue 2076 -- a `[[bridge]]`'s runtime-created rows, expanded HERE so
    // every consumer of this composition (knobs, descriptor rows, fragments)
    // reads them, not only the derivation; from the runtime's own config when
    // the workspace has it.
    let runtime = workspace.and_then(|ws| bridge_runtime_for(model, &source, ws, who));
    let Some(contract) =
        crate::entity_inventory::EntityInventory::from_model(source.clone(), model)
    else {
        let Some(ws) = workspace else {
            return Ok(None);
        };
        let nano_ros = crate::orchestration::nano_ros_root::resolve(None, ws);
        let (inv, _read, notes, stale) =
            probe_inventory_with(source, ws, nano_ros.as_deref(), model);
        for n in notes {
            eprintln!("{who}: {n}");
        }
        if inv.is_none()
            && let Some(why) = stale_launched_reason(model, &stale)
        {
            return Err(why);
        }
        return Ok(inv.map(|i| i.with_bridge_rows(runtime.as_ref())));
    };
    let Some(ws) = workspace else {
        return Ok(Some(contract.with_bridge_rows(None)));
    };
    let (floored, _read, notes, stale) = floor_by_recorded(ws, model, &contract);
    for n in notes {
        eprintln!("{who}: {n}");
    }
    // A stale floor is a missing floor: the contract alone may under-count
    // what the code creates (issue 1694's reason for the floor).
    if let Some(why) = stale_launched_reason(model, &stale) {
        return Err(why);
    }
    Ok(Some(floored.with_bridge_rows(runtime.as_ref())))
}

/// [`contract_inventory`] for a composer that keeps going without a
/// derivation: a stale launched sidecar is reported under
/// `NROS_PROBE_FAILURE` (a warning, or under `refuse` an error), and with no
/// refusal the image is composed as if the model described no wiring.
// Issue 2734.
pub fn contract_inventory_reported(
    source: impl Into<String>,
    model: &nros_orchestration_ir::overlay::LoadedModel,
    workspace: Option<&Path>,
    who: &str,
) -> Result<Option<crate::entity_inventory::EntityInventory>> {
    let source = source.into();
    match contract_inventory(source.clone(), model, workspace, who) {
        Ok(inv) => Ok(inv),
        Err(why) => {
            crate::resolve::report_not_derived(
                "entity counts not derived",
                &source,
                &crate::resolve::NotDerived::Failed(why),
            )?;
            Ok(None)
        }
    }
}

/// Issue 2076 -- the bringup's generated `nros-bridge.toml` (`nros sync`'s
/// `generate_bridge_configs`), for a model whose overlay declares a bridge.
///
/// The bringup is the directory the model was resolved into
/// (`build/nros/models/<bringup>/`), found among the workspace's packages. A
/// model with no bridge reads nothing. A file that is missing or does not
/// parse is `None` with a note, which leaves a forward-all bridge unresolved
/// and the derivation refused.
fn bridge_runtime_for(
    model: &nros_orchestration_ir::overlay::LoadedModel,
    source: &str,
    ws: &Path,
    who: &str,
) -> Option<crate::entity_inventory::BridgeRuntime> {
    if model.overlay.bridges.is_empty() {
        return None;
    }
    let bringup = Path::new(source)
        .parent()?
        .file_name()?
        .to_str()?
        .to_string();
    let workspace = Workspace::discover(ws).ok()?;
    let pkg = workspace
        .packages
        .iter()
        .find(|p| p.name == bringup && p.system_toml.is_some())?;
    let path = pkg.root.join("nros-bridge.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "{who}: {}: {e}; the bridge's rows are not read",
                path.display()
            );
            return None;
        }
    };
    match crate::entity_inventory::BridgeRuntime::parse(&text) {
        Ok(rt) => Some(rt),
        Err(e) => {
            eprintln!(
                "{who}: {}: {e}; the bridge's rows are not read",
                path.display()
            );
            None
        }
    }
}

/// phase-517 W5 (RFC-0100 amendment of 2026-10-25, accepted 2026-10-26) --
/// the declaration of a model-road image that authored NO contract, read from
/// what the probe recorded its launched nodes' code creating.
///
/// The probe RUNS each component's constructor against the recording backend
/// and counts what it registers -- the statement [`floor_by_recorded`] already
/// floors a contract with (issue 1694). With no contract it is the only
/// statement there is, so it stands, as [`Declaration::Stated`] rows whose
/// provenance is [`StatedBy::Probe`] (descriptor `basis = "probe"`).
///
/// ALL OR NOTHING. `Some` only when every node the launch tree starts has
/// exactly one FRESH sidecar (the [`fresh_probe_rows`] rule
/// [`refresh_stale_sidecars`] and issue 1594's join trust). A stale, missing
/// or unreadable sidecar for any one of them answers `None` -- the image then
/// refuses exactly as it did before this existed, and keeps its configured
/// pool knobs. A partial answer would turn "no contract, no change" into a
/// descriptor whose rows describe some of the image.
///
/// What the probe cannot see is an entity created AFTER construction (a
/// publisher opened in a callback); such a component needs a contract row,
/// which then wins through [`contract_inventory`]'s other arm.
///
/// [`Declaration::Stated`]: crate::entity_inventory::Declaration::Stated
/// [`StatedBy::Probe`]: crate::entity_inventory::StatedBy::Probe
pub fn probe_inventory(
    source: impl Into<String>,
    ws_root: &Path,
    model: &nros_orchestration_ir::overlay::LoadedModel,
) -> (
    Option<crate::entity_inventory::EntityInventory>,
    Vec<PathBuf>,
    Vec<String>,
) {
    let nano_ros = crate::orchestration::nano_ros_root::resolve(None, ws_root);
    let (inv, read, notes, _) = probe_inventory_with(source, ws_root, nano_ros.as_deref(), model);
    (inv, read, notes)
}

/// [`probe_inventory`] against a named nano-ros root (the probe key's
/// closure), so a test can stand one up.
fn probe_inventory_with(
    source: impl Into<String>,
    ws_root: &Path,
    nano_ros: Option<&Path>,
    model: &nros_orchestration_ir::overlay::LoadedModel,
) -> (
    Option<crate::entity_inventory::EntityInventory>,
    Vec<PathBuf>,
    Vec<String>,
    Vec<StaleSidecar>,
) {
    if model.structure.nodes.is_empty() {
        return (None, Vec::new(), Vec::new(), Vec::new());
    }
    let (rows, read, mut notes, stale) = match probe_rows_and_stale(ws_root, nano_ros) {
        Ok(v) => v,
        Err(e) => {
            return (
                None,
                Vec::new(),
                vec![format!(
                    "no probe sidecar read ({e}); with no contract, the image keeps its \
                     configured pool knobs"
                )],
                Vec::new(),
            );
        }
    };
    let launched = model.structure.nodes.len();
    let inv = match crate::entity_inventory::EntityInventory::from_probe(source, model, &rows) {
        Ok(inv) => Some(inv),
        Err(unrecorded) => {
            // Said only when the probe answered for SOME node: an image no
            // probe reached at all is the ordinary contract-less state, and the
            // derivation's own refusal already names it.
            if !unrecorded.is_empty() && unrecorded.len() < launched {
                notes.push(format!(
                    "no contract, and {} of {launched} launched node(s) have no fresh probe \
                     sidecar ({}), so the probe does not describe this image and it keeps its \
                     configured pool knobs",
                    unrecorded.len(),
                    unrecorded.join(", ")
                ));
            }
            None
        }
    };
    notes.dedup();
    (inv, read, notes, stale)
}

/// One FRESH probe sidecar, as [`fresh_probe_rows`] read it.
#[derive(Debug, Clone)]
pub struct ProbeRow {
    /// The sidecar's `executable` -- with `entities.pkg`, the
    /// `(package, executable)` key a launch-tree node (`pkg` + `exec`) names
    /// it by, which is the key `sidecar_slots::slots_of_component` counts on.
    pub executable: Option<String>,
    pub entities: crate::entity_inventory::ComponentEntities,
    /// Issue 2650 -- the sidecar's `parameter_description_max_len`: the
    /// longest parameter description its code set while the probe watched.
    /// `None` when the key is absent, which means the probe did NOT watch
    /// (a sidecar from before schema v6), never "no description".
    pub description_max_len: Option<usize>,
}

/// The workspace's FRESH probe sidecars, one row each, under the freshness rule
/// [`fresh_probe_inventory`] documents -- that function and issue 1694's
/// count floor ([`crate::entity_inventory::EntityInventory::recorded_for_launch`])
/// read ONE set of sidecars through this.
///
/// The second value is every sidecar this CONSULTED -- fresh, stale or
/// unreadable -- because it is what a caller registers as configure inputs
/// (issue 1018), and the inputs must not depend on the state they track
/// (issue 2361). It used to be the fresh ones only, so a configure that found
/// the sidecars stale registered none of them, and the `nros configure` that
/// made them current again re-ran nothing: measured on the safety island, the
/// image stayed sized from the closure fallback (`registration_path` refused on
/// six subscriptions, `region 'DTCM' overflowed by 18336 bytes`) until a fresh
/// configure was forced. A sidecar that does not exist is not listed: a
/// configure input with no file is a ninja load error, and its directory is
/// not a safe stand-in (anything the configure writes there would re-date it
/// past `build.ninja`).
pub fn fresh_probe_rows(
    ws_root: &Path,
    nano_ros: Option<&Path>,
) -> Result<(Vec<ProbeRow>, Vec<PathBuf>, Vec<String>)> {
    probe_rows_and_stale(ws_root, nano_ros).map(|(rows, read, notes, _)| (rows, read, notes))
}

/// A probe sidecar that exists and is stale: the package it describes and why
/// it is stale (the moved input). The probe DID describe this package, so a
/// reader that loses its rows has lost a declaration, not found none.
// Issue 2734.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleSidecar {
    pub pkg: String,
    pub why: String,
}

/// What [`probe_rows_and_stale`] read: the fresh rows, every sidecar it
/// consulted, its notes, and the stale sidecars it skipped.
pub type ProbeRead = (Vec<ProbeRow>, Vec<PathBuf>, Vec<String>, Vec<StaleSidecar>);

/// [`fresh_probe_rows`], plus every stale sidecar it skipped, by package.
pub fn probe_rows_and_stale(ws_root: &Path, nano_ros: Option<&Path>) -> Result<ProbeRead> {
    use crate::entity_inventory::ComponentEntities;

    let workspace = Workspace::discover(ws_root)?;
    let mut notes = Vec::new();
    let mut read: Vec<PathBuf> = Vec::new();
    let mut rows: Vec<ProbeRow> = Vec::new();
    let mut stale: Vec<StaleSidecar> = Vec::new();
    // Issue 2640 -- the key the refresh probed under includes the launch values.
    let seed_models = SeedModels::load(ws_root, None);
    for decl in workspace.component_declarations()? {
        let sidecar = decl.source_metadata_path();
        if !sidecar.is_file() {
            continue;
        }
        // Issue 2361 -- an input whatever its state, before any `continue`.
        read.push(sidecar.clone());
        let fresh = component_probe_key(
            &decl.package_root,
            nano_ros,
            &seeding_of(&seed_models, &decl),
        )
        .is_some_and(|k| sidecar_is_fresh(&sidecar, &k));
        if !fresh {
            let note = format!(
                "{}: stale against its sources (re-run `nros configure`), so neither its counts nor \
                 its registration observations are used. Moved: {}",
                sidecar.display(),
                stale_key_attribution(
                    &sidecar,
                    &decl.package_root,
                    nano_ros,
                    &seeding_of(&seed_models, &decl),
                )
            );
            // The package the stale sidecar describes, from the sidecar itself
            // (it still parses: staleness is about its key, not its shape).
            let pkg = std::fs::read_to_string(&sidecar)
                .ok()
                .and_then(|raw| crate::leaf_entity_env::declaration_from_probe(&raw).ok())
                .map(|(pkg, _, _)| pkg)
                .or_else(|| {
                    decl.package_root
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                });
            if let Some(pkg) = pkg {
                stale.push(StaleSidecar {
                    pkg,
                    why: note.clone(),
                });
            }
            notes.push(note);
            continue;
        }
        let raw = std::fs::read_to_string(&sidecar)
            .wrap_err_with(|| format!("read {}", sidecar.display()))?;
        match crate::leaf_entity_env::declaration_from_probe(&raw) {
            Ok((pkg, component, declaration)) => {
                // Issue 2689 -- through the STRICT reader, never a lenient
                // `serde_json::Value`: one typed mirror of the sidecar, so a
                // key this CLI does not know fails here the same way it fails
                // the freshness check, instead of reading as absent. A fresh
                // sidecar has already parsed (`sidecar_is_fresh`), so this
                // only fails if it changed underfoot.
                let doc: SourceMetadata = serde_json::from_str(&raw).map_err(|e| {
                    eyre::eyre!(
                        "{} does not parse as SourceMetadata ({e})",
                        sidecar.display()
                    )
                })?;
                let executable = doc.executable;
                let description_max_len = doc
                    .parameter_description_max_len
                    .and_then(|n| usize::try_from(n).ok());
                rows.push(ProbeRow {
                    executable,
                    description_max_len,
                    entities: ComponentEntities {
                        pkg,
                        class: component.clone(),
                        component,
                        declaration,
                    },
                });
            }
            Err(e) => notes.push(format!("{}: {e}", sidecar.display())),
        }
    }
    Ok((rows, read, notes, stale))
}

/// The stale sidecars of packages `model` launches, as one failure reason, or
/// `None` when every launched package's sidecar is fresh or absent.
// Issue 2734 -- such an image's counts are declared and could not be read, so
// it is a failure to derive, never "nothing declared".
pub fn stale_launched_reason(
    model: &nros_orchestration_ir::overlay::LoadedModel,
    stale: &[StaleSidecar],
) -> Option<String> {
    let launched: std::collections::BTreeSet<&str> = model
        .structure
        .nodes
        .values()
        .filter_map(|n| n.pkg.as_deref())
        .collect();
    let hit: Vec<&StaleSidecar> = stale
        .iter()
        .filter(|s| launched.contains(s.pkg.as_str()))
        .collect();
    (!hit.is_empty()).then(|| {
        format!(
            "the probe's counts for {} launched component(s) are stale: {}",
            hit.len(),
            hit.iter()
                .map(|s| s.why.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )
    })
}

/// Issue 1639 — remove the sidecars of components no declaration names.
///
/// The probe writes one `<dir>/<component>.json` per DECLARED component, and
/// every reader of that directory — `leaf_entity_env::inventory_for_leaf`
/// (the cargo-leaf sizing descriptor), `Workspace::source_metadata_files`
/// (the planner, model ingest, `nros metadata`) — globs `*.json` in it. So a
/// rename left the old component's sidecar behind and the next sync composed
/// it into the image: measured on `bins/in-place-subscriptions` after
/// ~~`five_listener`~~ → `eight_listener`, 13 subscription rows for an image
/// registering 8, and `ARENA_SIZE = 162,936` where the image needs 10,240.
///
/// Pruned here, at the one writer, rather than filtered at four readers: the
/// directory then means what every reader already assumes it means. Only
/// within a directory a declared sidecar lives in, and only a file the PROBE
/// wrote — one carrying [`SourceMetadataProvenance`], which [`stamp_provenance`]
/// sets on every successful probe. A `*.json` without it (a hand-written test
/// fixture) is not this pass's to delete; it is reported instead, because the
/// readers still see it. An orphaned `.json.unprobeable` marker is always this
/// module's, and goes with its component.
fn prune_undeclared_sidecars(declarations: &[ComponentDeclaration], report: &mut RefreshReport) {
    use std::collections::BTreeSet;
    let declared: BTreeSet<PathBuf> = declarations
        .iter()
        .map(ComponentDeclaration::source_metadata_path)
        .collect();
    let dirs: BTreeSet<PathBuf> = declared
        .iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if let Some(json) = name.strip_suffix(".unprobeable") {
                if json.ends_with(".json") && !declared.contains(&dir.join(json)) {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            if !name.ends_with(".json") || declared.contains(&path) {
                continue;
            }
            let probe_wrote = std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<SourceMetadata>(&raw).ok())
                .is_some_and(|m| m.provenance.is_some());
            if probe_wrote && std::fs::remove_file(&path).is_ok() {
                let _ = std::fs::remove_file(unprobeable_marker(&path));
                report.pruned.push(path);
            } else if path.exists() {
                // `exists()` because several fixture rows of one leaf sync
                // concurrently (issue 0498): a sibling that pruned it first
                // is not a file this pass kept.
                report.undeclared_kept.push(path);
            }
        }
    }
}

/// A sidecar is fresh iff it parses AND its recorded provenance digest matches
/// the sources on disk. An unparseable or unstamped sidecar is stale by
/// definition — that is the "museum data" case, and rebuilding is the only
/// answer that cannot be wrong.
fn sidecar_is_fresh(sidecar: &Path, digest: &str) -> bool {
    let Ok(raw) = std::fs::read_to_string(sidecar) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<SourceMetadata>(&raw) else {
        return false;
    };
    parsed.provenance.is_some_and(|p| p.inputs_digest == digest)
}

/// Record the digest the sidecar was derived from. Done CLI-side rather than in
/// the harness so the stamp is one implementation for every producer language
/// (W3's C/C++ probe gets it for free), and so the round-trip through
/// [`SourceMetadata`] doubles as schema validation of what the harness emitted.
pub(crate) fn stamp_provenance(sidecar: &Path, digest: &str) -> Result<()> {
    stamp_probe(sidecar, digest, None)
}

/// [`stamp_provenance`], and the sidecar's `probe` object: the launch values a
/// C/C++ probe ran with, or why it ran unseeded (issue 2640, phase-566 D4).
pub(crate) fn stamp_probe(
    sidecar: &Path,
    digest: &str,
    probe: Option<ProbeSeedRecord>,
) -> Result<()> {
    let raw =
        std::fs::read_to_string(sidecar).wrap_err_with(|| format!("read {}", sidecar.display()))?;
    // Issue 2689 -- the serde error goes IN the message, not only in the
    // chain: it is what names the field (`unknown field
    // \`parameter_description_max_len\``), and a caller printing the top
    // context alone would otherwise say only "invalid".
    let mut parsed: SourceMetadata = serde_json::from_str(&raw).map_err(|e| {
        eyre::eyre!(
            "the probe sidecar at {} does not parse as this CLI's SourceMetadata ({e}); \
             a probe schema change the CLI's mirror (orchestration/source_metadata.rs) \
             has not caught up with",
            sidecar.display()
        )
    })?;
    parsed.provenance = Some(SourceMetadataProvenance {
        inputs_digest: digest.to_string(),
        generator: format!("nros {}", env!("CARGO_PKG_VERSION")),
    });
    parsed.probe = probe;
    let out = serde_json::to_string_pretty(&parsed)?;
    // Issue 0498 — atomic. Several fixture rows of ONE leaf (its zenoh / xrce /
    // cyclonedds coordinates) sync concurrently and contend for this path, which
    // is keyed by COMPONENT and so is not separated by the per-RMW target dirs.
    crate::atomic_file::atomic_write(sidecar, &out)
}

/// #0288 — the NEGATIVE cache, sibling to the sidecar. A package that DEGRADES
/// (a deploy-bound standalone example whose probe cannot build, or a C/C++
/// component that fails its probe) would otherwise be re-attempted — a full,
/// failing cargo/cmake build — on EVERY sync. Recording the failed source
/// digest lets the next sync skip it until its sources change, at which point
/// the digest differs and it is retried. Keyed by the SAME `source_digest` the
/// positive cache uses, so a fix (or a board becoming host-buildable) that
/// changes no example source still retries — because the marker is cleared on
/// any successful probe below.
fn unprobeable_marker(sidecar: &Path) -> PathBuf {
    PathBuf::from(format!("{}.unprobeable", sidecar.display()))
}

/// Has this component's probe already failed, at this exact key?
///
/// Issue 1578 retired ~~`unprobeable_key`~~ (the package digest plus the CLI stamp,
/// C/C++ only) into [`probe_inputs_key`], which every freshness decision now
/// shares. Its reason survives inside that function: a NEGATIVE marker keyed
/// on less than what could fix the probe hides a repair. What changed is that
/// the POSITIVE cache is held to the same standard — issue 0641 called a stale
/// positive sidecar "fine — caught by the coverage gate", and #1578 was one
/// that nothing caught.
fn is_known_unprobeable_now(
    sidecar: &Path,
    decl: &ComponentDeclaration,
    nano_ros: Option<&Path>,
    seeding: &Seeding,
) -> bool {
    component_probe_key(&decl.package_root, nano_ros, seeding)
        .is_some_and(|k| is_known_unprobeable(sidecar, &k, nano_ros))
}

/// Record a C/C++ probe failure against [`probe_inputs_key`], WITH its reason
/// (issue 1469 — the cause survives the cache that suppresses it).
///
/// `compile_root` is the tree the probe build dir's cache says it compiled
/// (issue 1593); `None` (no cache yet) falls back to `nano_ros`, which is what
/// the pinned configure asked for.
fn mark_unprobeable_now(
    sidecar: &Path,
    key: Option<&str>,
    nano_ros: Option<&Path>,
    compile_root: Option<&Path>,
    why: &str,
) {
    if let Some(key) = key {
        mark_unprobeable_with_reason(sidecar, key, compile_root.or(nano_ros), Some(why));
    }
}

/// What a sidecar is stamped with when no key could be formed. It never equals
/// a key [`probe_inputs_key`] returns (those are `fnv1a64:...` triples), so a
/// sidecar stamped with it is re-probed on the next sync — the "never fresh"
/// half of `None`.
pub(crate) const UNKEYED: &str = "unkeyed";

/// Issue 1593 — a marker is honoured only when its KEY matches AND the tree
/// its probe COMPILED (the `root:` line) is the tree this sync keys on. The key
/// describes the tree the CLI asked for; a cached probe build dir could compile
/// another one, and a failure recorded under the first key then stood for a
/// probe that never ran against it. A marker with no `root:` line predates the
/// rule and is not honoured either: it costs one re-probe, once.
fn is_known_unprobeable(sidecar: &Path, digest: &str, root: Option<&Path>) -> bool {
    let Ok(body) = std::fs::read_to_string(unprobeable_marker(sidecar)) else {
        return false;
    };
    marker_key(&body) == digest
        && match (marker_root(&body), root) {
            (Some(recorded), Some(want)) => same_tree(&recorded, want),
            _ => false,
        }
}

fn same_tree(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn marker_key(body: &str) -> &str {
    body.lines().next().unwrap_or("").trim()
}

const MARKER_ROOT: &str = "root: ";

/// The compile root a marker records — its second line, `root: <path>`.
fn marker_root(body: &str) -> Option<PathBuf> {
    body.lines()
        .nth(1)
        .and_then(|l| l.strip_prefix(MARKER_ROOT))
        .map(|p| PathBuf::from(p.trim()))
}

/// Why a prior sync gave up on this component, as it recorded it — `None` when
/// the marker predates issue 1469 or carried no reason.
fn recorded_unprobeable_reason(sidecar: &Path) -> Option<String> {
    let body = std::fs::read_to_string(unprobeable_marker(sidecar)).ok()?;
    let reason = body
        .lines()
        .skip(1)
        .filter(|l| !l.starts_with(MARKER_ROOT))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    (!reason.is_empty()).then_some(reason)
}

/// Record that the package's probe failed at `digest`, so it is not retried
/// until its sources change. Best-effort — a marker we cannot write just means
/// a wasted retry next sync, never a hard error.
///
/// The REASON is recorded beside the key because this skip is ABSORBING (issue
/// 1469). Once the marker exists, `nros sync` reports "probe failed at this
/// source last sync; unchanged" on every later run and never re-attempts, so the
/// compiler error that caused it is unreachable from any number of syncs — the
/// Autoware Safety Island's eight `E0428`s were recovered by deleting the marker
/// by hand. A cache entry that suppresses a diagnosis it does not record costs
/// every future reader the same excavation.
fn mark_unprobeable_with_reason(
    sidecar: &Path,
    digest: &str,
    compile_root: Option<&Path>,
    why: Option<&str>,
) {
    // Issue 1593 — no compile root, no marker: one that cannot say which tree
    // failed could only ever be honoured on faith.
    let Some(root) = compile_root else {
        return;
    };
    // Issue 2097 -- a failure that names an unprovisioned source is the HOST's,
    // not the inputs': its repair (`nros setup --source <name>`) changes
    // nothing the key covers, so a marker would outlive it and every later
    // sync would skip a probe that now passes. The image is then sized
    // without the sidecar (the safety island's DTCM overflowed by 2,548 B on
    // that bake) while the build's own warning says "run `nros sync`", which
    // could not help. No marker: the next sync retries, and fails fast and
    // loud again until the source is provisioned.
    if why.and_then(unprovisioned_source).is_some() {
        clear_unprobeable(sidecar);
        return;
    }
    let mut body = format!("{digest}\n{MARKER_ROOT}{}\n", root.display());
    if let Some(line) = why.map(first_error_line).filter(|l| !l.is_empty()) {
        body.push_str(&line);
        body.push('\n');
    }
    // Issue 0498 — atomic, like the sidecar beside it: `is_known_unprobeable`
    // compares the marker's KEY against a digest, so a concurrent reader catching
    // a truncated one reads "not unprobeable" and pays the full failing probe
    // this marker exists to skip.
    let _ = crate::atomic_file::atomic_write(&unprobeable_marker(sidecar), &body);
}

/// The one line worth carrying up from a probe failure.
///
/// A probe failure's text is the component's whole build log — `run_step`
/// surfaces stdout AND stderr precisely so the useful part is not lost — which
/// is right for the sync that HITS it and far too much to store in a marker, or
/// to print on every later sync. So the marker keeps the first line that names
/// an error, which for a compiler is the line that says what is wrong:
/// `error[E0428]: the name … is defined multiple times`, or `error: static
/// assertion failed: NROS_UNBOUNDED__…`.
///
/// Falls back to the message's own first line (the `metadata probe <step> failed
/// for …` frame) when nothing matches, so a failure that is not a compile still
/// records something a reader can act on rather than nothing at all.
///
/// Issue 1898 — a cargo error keeps the root cause of its `Caused by:` chain,
/// and the cap shortens the head rather than cutting that cause off: a
/// `failed to get <dep>` line is ~200 characters of package path before the
/// part that says why.
fn first_error_line(why: &str) -> String {
    use nros_cargo_profile::diagnostic::{elide_keeping_cause, first_error_with_cause};
    const CAP: usize = 300;
    let pick = first_error_with_cause(why, |l| {
        let lower = l.to_ascii_lowercase();
        lower.starts_with("error")
            || lower.contains("error:")
            || lower.contains("error[")
            // CMake's own shape, `CMake Error at <file>:<line> (<cmd>):`, has
            // none of the three, so a configure failure recorded only the
            // `failed for `workspace` (exit 1):` header and every later sync
            // printed an empty "last error".
            || lower.starts_with("cmake error")
            // A LINK failure: the linker's own lines say what is wrong and
            // contain no "error", while `collect2: error: ld returned 1 exit
            // status` (which does) only says that it failed. It comes after
            // them, so these keep the marker on the cause: the
            // qemu-armv7a-nuttx typed-carrier probe recorded nothing but the
            // collect2 line, an empty failure record for an undefined
            // `__nros_c_component_<pkg>_create`.
            || lower.contains("undefined reference to")
            || lower.contains("multiple definition of")
            || lower.contains("cannot find -l")
            || lower.contains("undefined symbol:")
    })
    .or_else(|| {
        why.lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    })
    .unwrap_or_default();
    elide_keeping_cause(&with_cmake_message(why, pick), CAP)
}

/// A CMake error is a HEADER (`CMake Error at <file>:<line> (message):`) whose
/// text is on the indented lines below it, so keeping the header alone records
/// where it failed and not why. The record of a probe refused for a
/// `project()` that did not match its `package.xml` read only
/// `CMake Error at …/NanoRosPackageXml.cmake:96 (message):`. Append those
/// lines, joined, up to the first blank or unindented one.
fn with_cmake_message(why: &str, pick: String) -> String {
    if !pick.to_ascii_lowercase().starts_with("cmake error") {
        return pick;
    }
    let lines: Vec<&str> = why.lines().collect();
    let Some(at) = lines.iter().position(|l| l.trim() == pick) else {
        return pick;
    };
    let body: Vec<&str> = lines[at + 1..]
        .iter()
        .take_while(|l| !l.trim().is_empty() && l.starts_with(char::is_whitespace))
        .map(|l| l.trim())
        .collect();
    if body.is_empty() {
        pick
    } else {
        format!("{pick} {}", body.join(" "))
    }
}

/// Issue 2097 -- the source a probe failure says to provision, `None` when it
/// names none.
///
/// Every "not provisioned" refusal in the tree names its remedy the same way:
/// the zenoh-pico and mbedTLS build scripts (`Run: nros setup --source
/// zenoh-pico`), `nros sdk-path`, and the Rust probe's own index-driven remedy
/// (#0390). So the remedy text is the test, not a per-source path list. A
/// false match costs one re-probe per sync, never a wrong sidecar.
fn unprovisioned_source(why: &str) -> Option<&str> {
    const REMEDY: &str = "nros setup --source ";
    let at = why.find(REMEDY)? + REMEDY.len();
    let rest = why[at..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(rest.len());
    let name = rest[..end].trim_end_matches('.');
    (!name.is_empty()).then_some(name)
}

/// Drop the negative marker — the package probed successfully (or its sidecar
/// is now current), so a stale "unprobeable" must not shadow it.
fn clear_unprobeable(sidecar: &Path) {
    let _ = std::fs::remove_file(unprobeable_marker(sidecar));
}

/// Content digest of every source file under a package root.
///
/// FNV-1a over `(relative path, bytes)` for each file in sorted order, mixed
/// with the CLI version (a harness change must invalidate every sidecar). No
/// hash crate is pulled in for this: the digest guards a rebuild decision, not
/// a security boundary, and a collision costs one stale sidecar that the W5
/// coverage gate would catch.
/// Issue 1578 — THE freshness key for a metadata probe's sidecar: positive and
/// negative, Rust and C/C++.
///
/// What a probe REPORTS is decided by three things, and this is all three:
///
/// 1. the component's own sources — [`source_digest`], what the key used to be
///    alone;
/// 2. the CLI that wrote the harness — `NROS_CLI_PLANNING_STAMP`, baked from
///    the CLI's own sources by `build.rs`, because `CARGO_PKG_VERSION` (the only
///    CLI input `source_digest` mixes) moves on a release and not on a fix.
///    The PLANNING stamp (issue 2441), not the source stamp: the harness and
///    the probe driver are planning code (`cli-planning-set.txt` holds every
///    file `nros configure` reaches), and the source stamp also moves for a
///    leaf verb such as `nros doctor`, which re-probed every sidecar and so
///    re-ran every `nros configure` over an edit that changes neither;
/// 3. the nano-ros crates the probe COMPILES — [`probe_closure_digest`] over
///    `packages/cli/probe-source-dirs.txt`. This is the half #1578 was about:
///    the `in_place` flag a sidecar carries comes from `nros-node`, and a change
///    there left the sidecar "already current". Measured, it reused
///    `in_place: false` against a source saying `true`.
///
/// Before this there were THREE spellings of "the probe's inputs": the plain
/// digest (Rust, positive and negative), and digest + CLI stamp (C/C++,
/// negative only — issue 0641, whose comment called a stale POSITIVE sidecar
/// "fine — caught by the coverage gate"; #1578 is the counterexample, a stale
/// sidecar nothing caught).
///
/// `None` when there is no nano-ros path or no closure list, and `None` is
/// NEVER fresh: the sidecar is re-probed rather than trusted over a smaller key.
/// That is `cli-source-dirs.txt`'s rule for the CLI's own stamp (issue 0627).
pub fn probe_inputs_key(package_root: &Path, nano_ros: Option<&Path>) -> Option<String> {
    let parts = probe_inputs_parts(package_root, nano_ros);
    if parts.iter().any(|(_, v)| v.is_err()) {
        return None;
    }
    let v = |i: usize| parts[i].1.as_deref().unwrap_or_default().to_string();
    Some(format!("{}:{}:{}:{}{}", v(0), v(1), v(2), v(3), v(4)))
}

/// The labelled inputs [`probe_inputs_key`] folds, in key order. An `Err`
/// names why that input could not be computed (which makes the key `None`).
///
/// Issue 2720 -- one list for the key and for [`stale_key_attribution`], so
/// the diagnostic cannot name an input the key does not contain.
fn probe_inputs_parts(
    package_root: &Path,
    nano_ros: Option<&Path>,
) -> [(&'static str, Result<String, String>); 5] {
    let pkg = source_digest(package_root).map_err(|e| format!("{e}"));
    let closure = match nano_ros {
        None => Err("no nano-ros checkout resolved for the probe closure".to_string()),
        Some(root) => probe_closure_digest(root).ok_or_else(|| {
            format!(
                "no probe closure list at {}",
                root.join(PROBE_SOURCE_DIRS).display()
            )
        }),
    };
    // Issue 2680 -- and the workspace packages the probe adds before this one:
    // their names and sources. Empty without any, so such a key is unchanged.
    let deps = super::probe_deps::key_field(package_root)
        .ok_or_else(|| "the workspace packages this one depends on could not be digested".into());
    // Issue 2640 -- and the knobs the probe's build pins (store and executor
    // sizes): a sidecar from a probe built under other values is not this
    // probe's answer.
    [
        ("component sources", pkg),
        (
            "CLI planning stamp",
            Ok(env!("NROS_CLI_PLANNING_STAMP").to_string()),
        ),
        ("nano-ros crates the probe compiles", closure),
        (
            "probe knobs",
            Ok(super::metadata_probe_cmake::probe_knob_key()),
        ),
        ("workspace dependencies", deps),
    ]
}

/// Issue 2720 -- say WHICH probe input makes a sidecar stale, with both
/// values, instead of one opaque "stale". The recorded digest is the key's
/// own text, so it splits back into the same five fields (issue 1018's
/// per-input attribution, applied to the probe key). Falls back to both whole
/// values when the recorded text does not split, e.g. one written under an
/// older key layout.
pub(crate) fn stale_key_attribution(
    sidecar: &Path,
    package_root: &Path,
    nano_ros: Option<&Path>,
    seeding: &Seeding,
) -> String {
    let recorded = std::fs::read_to_string(sidecar)
        .ok()
        .and_then(|raw| serde_json::from_str::<SourceMetadata>(&raw).ok())
        .and_then(|m| m.provenance)
        .map(|p| p.inputs_digest);
    let Some(recorded) = recorded else {
        return "it records no probe-input digest".into();
    };
    let base = probe_inputs_parts(package_root, nano_ros);
    // Issue 2640's sixth input, appended by `component_probe_key`.
    let seeded = (
        "launch values the probe is seeded with",
        Ok(seeding
            .digest()
            .map(|d| format!(":{d}"))
            .unwrap_or_default()),
    );
    let parts: Vec<(&str, Result<String, String>)> =
        base.into_iter().chain(std::iter::once(seeded)).collect();
    let unknown: Vec<String> = parts
        .iter()
        .filter_map(|(label, v)| v.as_ref().err().map(|why| format!("{label}: {why}")))
        .collect();
    if !unknown.is_empty() {
        return format!(
            "its current key cannot be computed ({})",
            unknown.join("; ")
        );
    }
    let Some(old) = split_recorded_key(&recorded) else {
        let now = component_probe_key(package_root, nano_ros, seeding).unwrap_or_default();
        return format!("recorded `{recorded}`, current `{now}`");
    };
    let moved: Vec<String> = parts
        .iter()
        .zip(old.iter())
        .filter_map(|((label, now), was)| {
            let now = now.as_deref().unwrap_or_default();
            (now != was).then(|| format!("{label}: recorded `{was}`, now `{now}`"))
        })
        .collect();
    if moved.is_empty() {
        format!("recorded `{recorded}` differs only in layout from the current key")
    } else {
        moved.join("; ")
    }
}

/// The six fields of a recorded key: `fnv1a64:<h>`, planning stamp,
/// `fnv1a64:<h>`, knobs, an optional `:deps=...` and an optional
/// `:sha256:<h>` seeding suffix (issue 2640).
fn split_recorded_key(recorded: &str) -> Option<[String; 6]> {
    let mut it = recorded.splitn(6, ':');
    let (a, b, plan, c, d, rest) = (
        it.next()?,
        it.next()?,
        it.next()?,
        it.next()?,
        it.next()?,
        it.next()?,
    );
    let (rest, seed) = match rest.rfind(":sha256:") {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (knobs, deps) = match rest.find(":deps=") {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    Some([
        format!("{a}:{b}"),
        plan.to_string(),
        format!("{c}:{d}"),
        knobs.to_string(),
        deps.to_string(),
        seed.to_string(),
    ])
}

/// Issue 2640 (phase-566 D4) -- one component's WHOLE probe key: the
/// package's [`probe_inputs_key`] and, when launch instances name the
/// component, the `sha256` digest of the values it is probed with. A component
/// no launch names keeps the key it had before seeds existed. Every freshness
/// decision on a component (the refresh's, its failure marker's, and every
/// reader that trusts a sidecar) asks this, so an edited parameter file
/// re-probes and a sidecar probed under other values is never read as current.
pub fn component_probe_key(
    package_root: &Path,
    nano_ros: Option<&Path>,
    seeding: &Seeding,
) -> Option<String> {
    let base = probe_inputs_key(package_root, nano_ros)?;
    Some(match seeding.digest() {
        Some(d) => format!("{base}:{d}"),
        None => base,
    })
}

/// Issue 2640 -- what `decl`'s probe is seeded with (phase-566 D2). A Rust
/// component is probed by the cargo harness, which takes no seeds yet
/// (phase-566 W4), so it is never seeded and its key is unchanged.
pub fn seeding_of(models: &SeedModels, decl: &ComponentDeclaration) -> Seeding {
    if decl.config.language == ComponentLanguage::Rust {
        return Seeding::None;
    }
    models.seeding(
        &decl.config.package,
        &decl
            .config
            .linkage
            .resolved_executable(&decl.config.component),
        decl.class.as_deref(),
    )
}

/// The generated list of crates a probe compiles, relative to a nano-ros root.
/// Written by `scripts/gen-probe-source-dirs.py`, gated by
/// `check-probe-source-dirs`.
pub const PROBE_SOURCE_DIRS: &str = "packages/cli/probe-source-dirs.txt";

/// FNV-1a over every source file under each directory the probe compiles, in
/// the checkout the probe compiles AGAINST (`nano_ros`, the same root the
/// harness is pointed at) — never this process's own tree.
///
/// Uncached on purpose: one call per component per sync, and a process-wide
/// cache would hand a caller that edits a file mid-process (a test, or a
/// long-running driver) the key of a tree that no longer exists — the very
/// failure this function closes.
///
/// `None` for a missing or empty list. An empty closure is a broken list, not
/// "nothing to watch": `nros` is always in its own closure.
pub fn probe_closure_digest(nano_ros: &Path) -> Option<String> {
    let list = std::fs::read_to_string(nano_ros.join(PROBE_SOURCE_DIRS)).ok()?;
    let dirs: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if dirs.is_empty() {
        return None;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mix = |bytes: &[u8], hash: &mut u64| {
        for b in bytes {
            *hash ^= u64::from(*b);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for d in dirs {
        let root = nano_ros.join(d);
        let mut files = Vec::new();
        collect_sources(&root, &root, &mut files).ok()?;
        files.sort();
        // The DIR name is mixed too, so a crate that moves is a different key
        // even if its bytes are identical.
        mix(d.as_bytes(), &mut hash);
        for rel in &files {
            mix(rel.to_string_lossy().as_bytes(), &mut hash);
            // Same tolerance as `source_digest`: a file that vanishes between
            // the walk and the read is build output a concurrent cargo removed.
            let Ok(bytes) = std::fs::read(root.join(rel)) else {
                continue;
            };
            mix(&bytes, &mut hash);
        }
    }
    Some(format!("fnv1a64:{hash:016x}"))
}

pub fn source_digest(package_root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_sources(package_root, package_root, &mut files)?;
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mix = |bytes: &[u8], hash: &mut u64| {
        for b in bytes {
            *hash ^= u64::from(*b);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    mix(env!("CARGO_PKG_VERSION").as_bytes(), &mut hash);
    for rel in &files {
        mix(rel.to_string_lossy().as_bytes(), &mut hash);
        // A file that vanishes between the walk and the read is build output a
        // concurrent cargo removed, not a source. `is_build_output_dir` should
        // already have excluded it — but with `incremental = true` (the
        // development profile since phase-336) cargo churns `.o` files under
        // dirs the walk has just listed, and a hard failure here took the whole
        // fixture build down. A genuinely missing SOURCE cannot disappear here
        // without also being absent from the walk.
        let Ok(bytes) = std::fs::read(package_root.join(rel)) else {
            continue;
        };
        mix(&bytes, &mut hash);
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

fn collect_sources(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if is_build_output_dir(&name) {
                continue;
            }
            collect_sources(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

/// phase-530 W1 (Ruling R45) -- THE nano-ros root a project's probe sidecars
/// are keyed against ([`probe_inputs_key`]): the explicit `--nano-ros-path`,
/// else the one ladder. Sync's refresh stamps with it, the configure stamp
/// checks with it, and `nros build`'s resolve reads with it, so the three
/// can never key one sidecar against two roots.
pub fn probe_nano_ros_root(project: &Path, explicit: Option<PathBuf>) -> Option<PathBuf> {
    crate::orchestration::nano_ros_root::resolve(explicit, project)
}

/// phase-530 W1 -- the sidecars under `root` the resolve would refuse as
/// stale, as one line each. Freshness is the resolve's own
/// predicate ([`fresh_probe_rows`]) — [`sidecar_is_fresh`] against the WHOLE
/// [`probe_inputs_key`] (component sources, this CLI's source stamp, the
/// crates the probe compiles under `nano_ros`). Comparing the source part
/// alone left a sidecar a rebuilt CLI had staled reading as current, so
/// configure said "up to date" while the resolve dropped every sidecar.
///
/// A component with no sidecar yet is not listed (nothing recorded, nothing
/// stale); an unreadable or unstamped one is. Ruling R5: a sidecar stamped
/// [`UNKEYED`] (no key could be formed when it was probed) has nothing to
/// compare, so it is not listed either.
pub(crate) fn stale_sidecar_components(root: &Path, nano_ros: Option<&Path>) -> Vec<String> {
    let Ok(workspace) = Workspace::discover(root) else {
        return Vec::new();
    };
    let Ok(declarations) = workspace.component_declarations() else {
        return Vec::new();
    };
    let tables_needed = probe_tables_needed(root, &declarations);
    let mut out = Vec::new();
    // Issue 2640 -- the refresh's key includes the launch values.
    let seed_models = SeedModels::load(root, None);
    for decl in &declarations {
        let sidecar = decl.source_metadata_path();
        let Ok(raw) = std::fs::read_to_string(&sidecar) else {
            continue;
        };
        let unkeyed = serde_json::from_str::<SourceMetadata>(&raw)
            .ok()
            .and_then(|m| m.provenance)
            .is_some_and(|p| p.inputs_digest == UNKEYED);
        let fresh = unkeyed
            || component_probe_key(
                &decl.package_root,
                nano_ros,
                &seeding_of(&seed_models, decl),
            )
            .is_some_and(|k| sidecar_is_fresh(&sidecar, &k));
        if !fresh {
            out.push(format!(
                "metadata sidecar `{}` is stale. Moved: {}",
                sidecar.display(),
                stale_key_attribution(
                    &sidecar,
                    &decl.package_root,
                    nano_ros,
                    &seeding_of(&seed_models, decl),
                )
            ));
        } else if decl.config.language != ComponentLanguage::Rust
            && !probe_build_present(
                &root.join("build").join("nros-metadata"),
                tables_needed.as_deref(),
            )
        {
            // Issue 2435 -- the same predicate the refresh applies: a C/C++
            // sidecar is current only beside the probe build that holds its
            // bound tables.
            out.push(format!(
                "metadata sidecar `{}` is not backed by a complete probe build: \
                 `build/nros-metadata` lacks the probe aggregate or a bound table the \
                 message-bound seeds read",
                sidecar.display()
            ));
        }
    }
    out
}

/// Newest mtime under a package root — diagnostics only (`nros ws status`).
/// Never a freshness input; see the module docs on why mtimes were rejected.
pub fn newest_source_mtime(package_root: &Path) -> Option<SystemTime> {
    let mut files = Vec::new();
    collect_sources(package_root, package_root, &mut files).ok()?;
    files
        .iter()
        .filter_map(|rel| {
            std::fs::metadata(package_root.join(rel))
                .ok()?
                .modified()
                .ok()
        })
        .max()
}

/// phase-463 W4 -- the recorder schema an entity census in THIS tree is read
/// against.
///
/// The SSoT is `nros::node_metadata::SOURCE_METADATA_SCHEMA_VERSION` in
/// `packages/api/nros/src/node_metadata.rs`; the two workspaces do not share a
/// crate, so the number is restated here and pinned by
/// `the_recorder_schema_version_matches_the_recorder` below rather than left
/// to drift. A census whose recorder schema is not this one is STALE by
/// definition and not "a document with a field I skipped" -- issue 0427's rule
/// for the resolver pin, applied to the recorder: the producer changed, so the
/// output would too, for byte-identical inputs.
pub const RECORDER_SCHEMA_VERSION: u32 = 6;

/// One input a derived artifact recorded, as it was recorded.
///
/// The digest carries its own algorithm (`sha256:` / `fnv1a64:`) because the
/// two kinds of input are hashed by different means for different reasons: a
/// FILE by content, a SOURCE TREE by [`source_digest`], the walk that already
/// decides when a sidecar is stale. [`recompute_digest`] dispatches on the
/// prefix rather than on the role, so a producer that adds a role does not
/// have to teach this reader anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedInput {
    pub role: String,
    /// Relative to the root the artifact was produced from.
    pub path: String,
    pub digest: String,
}

/// One recorded input that no longer describes what is on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleInput {
    pub role: String,
    pub path: String,
    pub recorded: String,
    /// What the input hashes to now, or why it could not be hashed.
    pub current: String,
}

impl StaleInput {
    /// The one line a refusal prints per stale input.
    pub fn line(&self) -> String {
        format!(
            "{} `{}` changed since the census was taken (recorded {}, now {})",
            self.role, self.path, self.recorded, self.current
        )
    }
}

/// Recompute ONE recorded digest against the path it names.
///
/// Content-addressed, never mtime-based -- the rule this module's header
/// states and the reason it states it. `touch` on a source file leaves both
/// answers here unchanged, which is the property phase-463 W4's gate exists to
/// hold: only a CHANGE to what the code says makes a census stale.
pub fn recompute_digest(path: &Path, recorded: &str) -> Result<String, String> {
    if recorded.starts_with("fnv1a64:") {
        // A SOURCE TREE. `source_digest` walks it, skipping build output and
        // the sidecar dir, and mixes this crate's version -- so a CLI that
        // hashes differently also reads as stale, which is correct.
        if !path.is_dir() {
            return Err(format!("`{}` is not a directory any more", path.display()));
        }
        return source_digest(path).map_err(|e| format!("{e}"));
    }
    if recorded.starts_with("sha256:") {
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
        return Ok(format!("sha256:{:x}", sha2::Sha256::digest(&bytes)));
    }
    if recorded.starts_with(CAPABILITIES_DIGEST_PREFIX) {
        return capabilities_digest(path);
    }
    Err(format!(
        "unknown digest algorithm in `{recorded}` -- the census was written by a producer this          tree cannot verify"
    ))
}

/// The digest algorithm of a census's `capabilities` input (issue 1680).
pub const CAPABILITIES_DIGEST_PREFIX: &str = "capabilities:";

/// The capability axes a bringup's `system.toml` turns on, as a census
/// freshness digest: `capabilities:<axis>,<axis>` in registry order, or
/// `capabilities:` for none.
///
/// Issue 1680 -- the axes are a BUILD input of the census binary, not a
/// statement about the contract: `param_services` and `lifecycle` register
/// their service families through the RMW, which the recorder sees and
/// `census_callback_slots` counts, so a census taken under one set is not a
/// statement about an image built under another. Only the AXES are digested,
/// never the whole file: `system.toml` also holds `[census.waive]` and the
/// `[census]` policy, and editing either must not stale the evidence they are
/// applied to (the rule `is_freshness_input` states for the contract).
///
/// Readable rather than hashed, so a stale verdict says which axis moved.
/// Axes are read the one way the build reads them,
/// `SystemToml::capability_enabled` over the registry (the `[system]
/// features` list and the deprecated typed blocks), so this cannot disagree
/// with the build about what is on. A file that does not parse is an error,
/// which the caller reports as STALE rather than as "unchanged".
pub fn capabilities_digest(system_toml: &Path) -> Result<String, String> {
    let raw = nros_system_include::read_to_string(system_toml)
        .map_err(|e| format!("cannot read `{}`: {e}", system_toml.display()))?;
    let sys: super::cargo_metadata_schema::SystemToml = toml::from_str(&raw)
        .map_err(|e| format!("cannot parse `{}`: {e}", system_toml.display()))?;
    let axes: Vec<&str> = cargo_nano_ros::capability_resolver::CAPABILITIES
        .iter()
        .filter(|c| sys.capability_enabled(c.declared))
        .map(|c| c.declared)
        .collect();
    Ok(format!("{CAPABILITIES_DIGEST_PREFIX}{}", axes.join(",")))
}

/// Every recorded input that is no longer current, in the order recorded.
///
/// An input that cannot be hashed at all (gone, or a tree that is now a file)
/// is STALE and not skipped: "I could not check" and "it still matches" are
/// the two answers a freshness gate may never confuse, which is the whole
/// defect issue 1419 is about one layer up.
pub fn stale_recorded_inputs(root: &Path, inputs: &[RecordedInput]) -> Vec<StaleInput> {
    let mut out = Vec::new();
    for input in inputs {
        let path = root.join(&input.path);
        let current = match recompute_digest(&path, &input.digest) {
            Ok(d) => d,
            Err(why) => why,
        };
        if current != input.digest {
            out.push(StaleInput {
                role: input.role.clone(),
                path: input.path.clone(),
                recorded: input.digest.clone(),
                current,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 2608 — a failed probe fails the command and names the node, its
    /// first error and where the full error is; the line a reader needs is the
    /// cause, not the `failed for` header above it.
    #[test]
    fn a_failed_probe_is_an_error_naming_node_and_cause() {
        let report = RefreshReport {
            failed: vec![ProbeFailure::new(
                "talker_pkg::talker".into(),
                "metadata probe build failed for `talker_pkg::talker` (exit 2):\n  \
                 Compiling zpico-sys v0.6.0\n  error: failed to run custom build command \
                 for `zpico-sys v0.6.0`",
                Some(PathBuf::from("m/talker.json.unprobeable")),
            )],
            ..Default::default()
        };
        let err = format!(
            "{:#}",
            report
                .check_failures(ProbeFailurePolicy::Refuse)
                .unwrap_err()
        );
        assert!(err.contains("1 metadata probe(s) failed"), "{err}");
        assert!(err.contains("talker_pkg::talker: "), "{err}");
        assert!(
            err.contains("error: failed to run custom build command"),
            "{err}"
        );
        assert!(err.contains("m/talker.json.unprobeable"), "{err}");
        assert!(!err.contains("no producer"), "{err}");
    }

    /// Issue 2608 — the negative control: a component with no producer, and a
    /// deploy-bound example whose host probe failed (#0288's designed
    /// fallback), are reported and NOT fatal.
    #[test]
    fn no_producer_and_deploy_bound_degrade_are_not_failures() {
        let report = RefreshReport {
            unsupported: vec!["pkg::node (no producer)".into()],
            degraded: vec!["ex::talker (deploy-bound probe failed: x)".into()],
            ..Default::default()
        };
        assert!(report.check_failures(ProbeFailurePolicy::Refuse).is_ok());
        assert!(report.check_failures(ProbeFailurePolicy::Warn).is_ok());
        assert_eq!(report.failure_summary(), None);
    }

    /// Issue 2680 -- a component dropped because a workspace DEPENDENCY the
    /// probe added could not configure is a failure like any other: it names
    /// the dependency and the CMake error, and the message still closes with
    /// the probe-args note, since a path such a dependency needs goes in the
    /// probe's one `[image_defaults.build] cmake_args`.
    #[test]
    fn a_dependency_blamed_failure_closes_with_the_probe_args_note() {
        use crate::orchestration::metadata_probe_cmake::{PROBE_ARGS_RULE, blame_reason};
        let why = blame_reason(
            Some("autoware_controller_libs"),
            "metadata probe configure failed for `workspace` (exit 1):\n\
             CMake Error at /ws/src/autoware/CMakeLists.txt:3 (find_package):\n\
               Could not find a package configuration file provided by \"Eigen3\"",
        );
        let report = RefreshReport {
            failed: vec![ProbeFailure::new(
                "controller_pkg::controller".into(),
                &why,
                None,
            )],
            ..Default::default()
        };
        let err = format!(
            "{:#}",
            report
                .check_failures(ProbeFailurePolicy::Refuse)
                .unwrap_err()
        );
        assert!(err.contains("controller_pkg::controller"), "{err}");
        assert!(
            err.contains("workspace dependency `autoware_controller_libs`"),
            "{err}"
        );
        assert!(
            err.ends_with(&format!("Note: {PROBE_ARGS_RULE}.")),
            "the probe-args note closes the message:\n{err}"
        );
    }

    fn one_failure() -> ProbeFailure {
        ProbeFailure::new("p::n".into(), "error: boom", None)
    }

    /// Issue 2608 — under `warn`, a failure beside a probe that worked is
    /// reported, not fatal, and the summary carries the count.
    #[test]
    fn warn_reports_a_partial_failure_and_counts_it() {
        let report = RefreshReport {
            rebuilt: vec![PathBuf::from("ok.json")],
            failed: vec![one_failure()],
            ..Default::default()
        };
        assert!(report.check_failures(ProbeFailurePolicy::Warn).is_ok());
        let line = report.failure_summary().expect("a summary");
        assert!(line.starts_with("1 metadata probe(s) FAILED"), "{line}");
        assert!(report.check_failures(ProbeFailurePolicy::Refuse).is_err());
    }

    /// Issue 2608's minimum — a run where EVERY probe failed refuses even
    /// under `warn`.
    #[test]
    fn warn_refuses_when_every_probe_failed() {
        let report = RefreshReport {
            failed: vec![one_failure(), one_failure()],
            ..Default::default()
        };
        let err = format!(
            "{:#}",
            report.check_failures(ProbeFailurePolicy::Warn).unwrap_err()
        );
        assert!(err.contains("2 metadata probe(s) failed"), "{err}");
        assert!(
            err.contains("No probe in this workspace succeeded"),
            "{err}"
        );
    }

    #[test]
    fn the_policy_reads_only_its_two_words() {
        assert_eq!(
            ProbeFailurePolicy::parse(None).unwrap(),
            ProbeFailurePolicy::Warn
        );
        assert_eq!(
            ProbeFailurePolicy::parse(Some("")).unwrap(),
            ProbeFailurePolicy::Warn
        );
        assert_eq!(
            ProbeFailurePolicy::parse(Some("refuse")).unwrap(),
            ProbeFailurePolicy::Refuse
        );
        assert!(ProbeFailurePolicy::parse(Some("strict")).is_err());
    }

    /// A probe configure that fails CMake's compiler check records CMake's
    /// error, not just the `failed for `workspace`` header — the shape CI
    /// printed as an empty "last error" for a ThreadX and a NuttX leaf.
    #[test]
    fn a_cmake_configure_error_is_the_recorded_reason() {
        let why = "metadata probe configure failed for `workspace` (exit 1):\n\
                   -- The C compiler identification is GNU 13.2.1\n\n\
                   CMake Error at /usr/share/cmake-3.22/Modules/CMakeTestCCompiler.cmake:69 (message):\n  \
                   The C compiler\n\n    \"/opt/arm-none-eabi-gcc\"\n\n  \
                   is not able to compile a simple test program.\n";
        let line = first_error_line(why);
        assert!(line.contains("CMake Error at"), "got {line:?}");
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = crate::test_support::scratch_dir(&format!("md-refresh-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    /// Issue 1680 -- the census's `capabilities` digest moves with the axes
    /// and with nothing else in the file.
    #[test]
    fn capabilities_digest_moves_with_the_axes_and_nothing_else() {
        let dir = tmp("capabilities-digest");
        let st = dir.join("system.toml");
        let base = "[system]\nname = \"t\"\nrmw = \"zenoh\"\ndomain_id = 0\n";
        std::fs::write(&st, base).unwrap();
        let none = capabilities_digest(&st).unwrap();
        // phase-556 W13 -- no feature is the stock node interfaces.
        assert_eq!(none, "capabilities:param_services,rosout");

        // The `[census]` policy and waivers live in the same file and are
        // applied TO the census; editing them must not stale it.
        std::fs::write(
            &st,
            format!("{base}\n[census]\non_missing = \"warn\"\non_stale = \"warn\"\n"),
        )
        .unwrap();
        assert_eq!(capabilities_digest(&st).unwrap(), none);

        // Both spellings of an axis read the same way the build reads them.
        std::fs::write(
            &st,
            base.replace(
                "domain_id = 0\n",
                "domain_id = 0\nfeatures = [\"lifecycle\", \"param_services\"]\n",
            ),
        )
        .unwrap();
        let listed = capabilities_digest(&st).unwrap();
        assert_eq!(listed, "capabilities:param_services,lifecycle,rosout");
        std::fs::write(&st, format!("{base}\n[safety]\nenabled = true\n")).unwrap();
        assert_eq!(
            capabilities_digest(&st).unwrap(),
            "capabilities:safety,param_services,rosout"
        );

        // And through the freshness path: recorded under none, read after the
        // edit, it is STALE and the line names both axis sets.
        let stale = stale_recorded_inputs(
            &dir,
            &[RecordedInput {
                role: "capabilities".into(),
                path: "system.toml".into(),
                digest: none.clone(),
            }],
        );
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(
            stale[0].line().contains("now capabilities:safety"),
            "{}",
            stale[0].line()
        );

        // A file that no longer parses is STALE, never "unchanged".
        std::fs::write(&st, "not = [toml").unwrap();
        assert_eq!(
            stale_recorded_inputs(
                &dir,
                &[RecordedInput {
                    role: "capabilities".into(),
                    path: "system.toml".into(),
                    digest: none,
                }],
            )
            .len(),
            1
        );
    }

    /// A probe-ready declaration in `dir`, carrying `language`.
    fn probeable_decl(dir: &Path, language: ComponentLanguage) -> ComponentDeclaration {
        use crate::orchestration::config::{
            ComponentConfig, ComponentLinkage, ComponentMetadataConfig, ComponentOverrides,
        };
        ComponentDeclaration {
            package_root: dir.to_path_buf(),
            manifest_path: dir.join("nros.toml"),
            class: Some("demo_pkg::Talker".into()),
            crate_name: None,
            deploy_bound: false,
            header: None,
            shape: None,
            library_target: Some("demo_pkg".into()),
            config: ComponentConfig {
                version: 1,
                package: "demo_pkg".into(),
                component: "demo_pkg::talker".into(),
                class: Some("demo_pkg::Talker".into()),
                language,
                linkage: ComponentLinkage::default(),
                metadata: ComponentMetadataConfig {
                    source_metadata: "metadata/talker.json".into(),
                    generated_by: None,
                },
                overrides: ComponentOverrides::default(),
            },
        }
    }

    /// Issue 1528 — the probe's language is decided by an exhaustive match,
    /// and the two reachable answers are unchanged.
    ///
    /// The negative direction is the point: C still probes as `c` and C++ as
    /// `cpp`. What the wildcard used to cover in addition was `Rust`, which
    /// the caller's `language != Rust` guard has always excluded — so the
    /// refusal changes nothing that runs, and makes a fourth `Language`
    /// variant a compile error here instead of a silent C++ probe.
    #[test]
    fn the_probe_language_is_decided_with_no_wildcard() {
        let dir = tmp("probe-lang");
        let nano_ros = dir.join("nano-ros");
        let probe_root = dir.join("probe");

        for language in [ComponentLanguage::C, ComponentLanguage::Cpp] {
            let opts = cpp_probe_options(
                &probeable_decl(&dir, language),
                Some(&nano_ros),
                &probe_root,
                &Seeding::None,
                Some(&[]),
            )
            .unwrap_or_else(|why| panic!("{language:?} must be probeable: {why}"))
            .unwrap_or_else(|| panic!("{language:?}: no sidecar exists, so a probe is needed"));
            assert_eq!(opts.language, language, "{language:?}");
        }

        let why = cpp_probe_options(
            &probeable_decl(&dir, ComponentLanguage::Rust),
            Some(&nano_ros),
            &probe_root,
            &Seeding::None,
            Some(&[]),
        )
        .expect_err("a Rust component has no cmake probe — it has the cargo harness");
        assert!(
            why.contains("cargo metadata harness"),
            "the refusal must say where a Rust component is produced instead: {why}"
        );
    }

    /// Issue 2435 -- a fresh sidecar is not a current probe when the probe's
    /// build is gone: the bound tables the message-bound seeds read live there,
    /// and `rm -rf build` removes them while the sidecar (in the source tree)
    /// survives. With the probe's build present, the same sidecar is current.
    /// Issue 2720 -- a stale sidecar names the input that moved, with both
    /// values, and a key the reader cannot compute says why. A recorded key
    /// splits back into the same fields the key is built from.
    #[test]
    fn a_stale_sidecar_names_the_input_that_moved() {
        let root = fake_nano_ros("attr-root");
        let dir = tmp("attr-pkg");
        std::fs::write(dir.join("src/lib.rs"), "// the component\n").unwrap();
        let decl = probeable_decl(&dir, ComponentLanguage::C);
        let sidecar = decl.source_metadata_path();
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
        std::fs::write(&sidecar, sidecar_body("demo_pkg::talker", false)).unwrap();
        let key = probe_inputs_key(&dir, Some(&root)).expect("keyed");
        let fields = split_recorded_key(&key).expect("a key splits into its fields");
        let parts = probe_inputs_parts(&dir, Some(&root));
        for ((label, now), was) in parts.iter().zip(fields.iter()) {
            assert_eq!(now.as_deref().unwrap(), was, "{label}");
        }
        assert_eq!(fields[5], "", "an unseeded key has no seeding field");
        // A seeded key splits too, the seed digest in its own field.
        let seeded_key = format!("{key}:sha256:00ff");
        let f = split_recorded_key(&seeded_key).expect("a seeded key splits");
        assert_eq!(f[5], ":sha256:00ff");
        assert_eq!(f[4], fields[4], "deps unchanged by the seed suffix");

        // The planning stamp the sidecar was written under is not this CLI's.
        let old = key.replacen(env!("NROS_CLI_PLANNING_STAMP"), "0123456789abcdef", 1);
        stamp_provenance(&sidecar, &old).unwrap();
        let why = stale_key_attribution(&sidecar, &dir, Some(&root), &Seeding::None);
        assert!(
            why.starts_with("CLI planning stamp: recorded `0123456789abcdef`"),
            "{why}"
        );
        assert!(
            !why.contains("component sources"),
            "only the moved input: {why}"
        );

        // The component's own source moved.
        stamp_provenance(&sidecar, &key).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "// edited\n").unwrap();
        let why = stale_key_attribution(&sidecar, &dir, Some(&root), &Seeding::None);
        assert!(
            why.starts_with("component sources: recorded `fnv1a64:"),
            "{why}"
        );

        // No nano-ros checkout: the reader cannot compute a key at all.
        let why = stale_key_attribution(&sidecar, &dir, None, &Seeding::None);
        assert!(why.contains("no nano-ros checkout resolved"), "{why}");
    }

    #[test]
    fn a_fresh_sidecar_without_the_probe_build_is_probed_again() {
        let root = fake_nano_ros("probe-build-root");
        let dir = tmp("probe-build-pkg");
        std::fs::write(dir.join("src/lib.rs"), "// the component\n").unwrap();
        let decl = probeable_decl(&dir, ComponentLanguage::C);
        let sidecar = decl.source_metadata_path();
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
        std::fs::write(&sidecar, sidecar_body("demo_pkg::talker", false)).unwrap();
        let key = probe_inputs_key(&dir, Some(&root)).expect("keyed");
        stamp_provenance(&sidecar, &key).unwrap();
        assert!(sidecar_is_fresh(&sidecar, &key));

        let probe_root = dir.join("probe");
        let needed = ["std_msgs".to_string()];
        let probe = |label: &str| {
            cpp_probe_options(
                &decl,
                Some(&root),
                &probe_root,
                &Seeding::None,
                Some(&needed),
            )
            .expect("probeable")
            .is_some()
            .then_some(())
            .ok_or(label.to_string())
        };
        assert!(
            probe("none").is_ok(),
            "no probe build here, so its bound tables are missing: the component must be probed"
        );

        let build =
            crate::orchestration::metadata_probe_cmake::probe_dir_for_workspace(&probe_root)
                .join("build");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("CMakeCache.txt"), "# configured\n").unwrap();
        // Issue 2721 -- a cache alone is a configure that STARTED here: a
        // failed build, or a removed `nros-codegen/`, leaves it behind.
        assert!(
            probe("cache only").is_ok(),
            "a CMake cache without the probe's outputs must not read as current"
        );
        std::fs::write(build.join("nros-metadata.json"), "{}").unwrap();
        assert!(
            probe("no table").is_ok(),
            "the aggregate without the bound table a seed reads must not read as current"
        );
        let table = build
            .join("nros-codegen/nano_ros_cpp/std_msgs")
            .join(rosidl_codegen::INVENTORY_JSON_NAME);
        std::fs::create_dir_all(table.parent().unwrap()).unwrap();
        std::fs::write(&table, "{}").unwrap();
        assert!(
            probe("all present").is_err(),
            "fresh sidecar, and the probe's outputs present: nothing to do"
        );
        // An unresolvable closure is never current.
        assert!(
            cpp_probe_options(&decl, Some(&root), &probe_root, &Seeding::None, None)
                .expect("probeable")
                .is_some()
        );
    }

    /// Issue 2280 -- a C/C++ package that is its own west application
    /// (`find_package(Zephyr)` in its CMakeLists.txt) is never handed to the
    /// host CMake probe, which would print Zephyr's "BOARD is not being
    /// defined" and fail the whole batch. An ament package, and a CMakeLists
    /// that names Zephyr only in a comment, still probe.
    #[test]
    fn a_west_application_is_not_handed_to_the_host_cmake_probe() {
        let dir = tmp("probe-west-leaf");
        let nano_ros = dir.join("nano-ros");
        let probe_root = dir.join("probe");
        let cmake = dir.join("CMakeLists.txt");

        std::fs::write(
            &cmake,
            "cmake_minimum_required(VERSION 3.20)\n\
             find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})\n\
             project(talker)\n",
        )
        .unwrap();
        for language in [ComponentLanguage::C, ComponentLanguage::Cpp] {
            let why = cpp_probe_options(
                &probeable_decl(&dir, language),
                Some(&nano_ros),
                &probe_root,
                &Seeding::None,
                Some(&[]),
            )
            .expect_err("a west application cannot be configured for the host");
            assert!(
                why.contains("find_package(Zephyr)") && why.contains("census"),
                "the refusal names the cause and what sizes the leaf instead: {why}"
            );
        }

        for text in [
            "cmake_minimum_required(VERSION 3.20)\nfind_package(ament_cmake REQUIRED)\n",
            "# unlike find_package(Zephyr), this is an ament package\nproject(demo_pkg)\n",
        ] {
            std::fs::write(&cmake, text).unwrap();
            cpp_probe_options(
                &probeable_decl(&dir, ComponentLanguage::Cpp),
                Some(&nano_ros),
                &probe_root,
                &Seeding::None,
                Some(&[]),
            )
            .unwrap_or_else(|why| panic!("an ament package still probes: {why}"))
            .expect("no sidecar exists, so a probe is needed");
        }
    }

    #[test]
    fn negative_cache_round_trip() {
        // #0288 — a deploy-bound probe failure is remembered by source digest,
        // retried when the source changes, and cleared on success.
        let dir = tmp("negcache");
        let sidecar = dir.join("metadata").join("comp.json");
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();

        assert!(
            !is_known_unprobeable(&sidecar, "d1", Some(&dir)),
            "no marker yet"
        );

        mark_unprobeable_with_reason(&sidecar, "d1", Some(&dir), None);
        assert!(
            is_known_unprobeable(&sidecar, "d1", Some(&dir)),
            "recorded at d1"
        );
        assert!(
            !is_known_unprobeable(&sidecar, "d2", Some(&dir)),
            "a source change (new digest) must retry, not stay skipped"
        );

        clear_unprobeable(&sidecar);
        assert!(
            !is_known_unprobeable(&sidecar, "d1", Some(&dir)),
            "a successful probe / fix clears the marker so it stops shadowing"
        );
    }

    /// issue 1469 — the marker carries the reason, and carrying it must not
    /// change what the marker is FOR. The skip never re-attempts, so a marker
    /// that records no cause makes the compiler error unreachable from any
    /// number of syncs.
    #[test]
    fn a_marker_records_why_and_still_keys_on_the_digest() {
        let dir = tmp("negcache-reason");
        let sidecar = dir.join("metadata").join("comp.json");
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();

        let log = "metadata probe build failed for `pkg::comp` (exit 2):\n\
                   \x20  Compiling nano-ros-cpp-ffi-nav_msgs v0.0.0\n\
                   error[E0428]: the name `builtin_interfaces_msg_time_t` is defined multiple times\n\
                   \x20 --> src/../../../pkg/nano_ros_cpp/…/types.rs:19:1\n";
        mark_unprobeable_with_reason(&sidecar, "d1", Some(&dir), Some(log));

        assert!(
            is_known_unprobeable(&sidecar, "d1", Some(&dir)),
            "the key is the FIRST LINE, so a reason must not break the skip"
        );
        assert!(
            !is_known_unprobeable(&sidecar, "d2", Some(&dir)),
            "a source change must still retry"
        );
        let why = recorded_unprobeable_reason(&sidecar).expect("a reason was recorded");
        assert_eq!(
            why, "error[E0428]: the name `builtin_interfaces_msg_time_t` is defined multiple times",
            "the recorded line must be the one that says what is wrong, not the frame around it"
        );

        // A marker with no reason still skips, and honestly reports that it
        // knows no reason (the `root:` line is not a reason).
        mark_unprobeable_with_reason(&sidecar, "d1", Some(&dir), None);
        assert!(
            is_known_unprobeable(&sidecar, "d1", Some(&dir)),
            "reason-less still matches"
        );
        assert_eq!(
            recorded_unprobeable_reason(&sidecar),
            None,
            "no reason recorded must read as None, never as an empty one"
        );
    }

    /// Issue 1593 — a marker records the tree its probe COMPILED, and is
    /// honoured only by a sync keyed on that tree. The key alone described the
    /// tree the CLI asked for, so a failure compiled against checkout B and
    /// keyed on checkout A skipped A's probe for ever.
    #[test]
    fn a_marker_from_another_compile_root_is_never_honoured() {
        let dir = tmp("negcache-root");
        let a = dir.join("checkout-a");
        let b = dir.join("checkout-b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let sidecar = dir.join("metadata").join("comp.json");
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();

        // The probe build dir compiled B; this sync's key is A's.
        mark_unprobeable_with_reason(&sidecar, "kA", Some(&b), Some("error: from B"));
        assert!(
            !is_known_unprobeable(&sidecar, "kA", Some(&a)),
            "B's failure must not stand for a probe that never ran against A"
        );
        assert!(
            is_known_unprobeable(&sidecar, "kA", Some(&b)),
            "B's own sync honours it"
        );
        assert_eq!(
            recorded_unprobeable_reason(&sidecar).as_deref(),
            Some("error: from B"),
            "the root line is not mistaken for the reason"
        );

        // A marker written before the rule (key + reason, no root) is not
        // honoured: it cannot say which tree failed.
        std::fs::write(unprobeable_marker(&sidecar), "kA\nerror: legacy\n").unwrap();
        assert!(
            !is_known_unprobeable(&sidecar, "kA", Some(&a)),
            "legacy marker"
        );

        // And no root, no marker at all.
        clear_unprobeable(&sidecar);
        mark_unprobeable_with_reason(&sidecar, "kA", None, Some("error: x"));
        assert!(
            !unprobeable_marker(&sidecar).exists(),
            "rootless marker written"
        );
    }

    /// Issue 2097 -- a probe that failed because a source was not provisioned
    /// leaves no marker, so the sync after `nros setup --source` probes again.
    /// The text is the island's real failure (zpico-sys's build script, inside
    /// the C/C++ batch's cmake log). A compile error beside it still records.
    #[test]
    fn an_unprovisioned_source_failure_leaves_no_marker() {
        let dir = tmp("negcache-unprovisioned");
        let sidecar = dir.join("metadata").join("comp.json");
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();

        let log = "metadata probe build failed for `autoware_mrm_handler::mrm_handler` (exit 2):\n\
                   error: failed to run custom build command for `zpico-sys v0.6.0 (/ws/packages/rmw/zenoh/zpico-sys)`\n\
                   \x20 thread 'main' panicked at /ws/packages/rmw/zenoh/nros-zpico-build/src/runner.rs:1731:9:\n\
                   \x20 zenoh-pico source not provisioned at \"/ws/packages/rmw/zenoh/zpico-sys/zenoh-pico\". \
                   Run: nros setup --source zenoh-pico (or git submodule update --init \
                   packages/rmw/zenoh/zpico-sys/zenoh-pico). -- #0390\n";
        assert_eq!(unprovisioned_source(log), Some("zenoh-pico"));
        assert_eq!(
            unprovisioned_source(
                "harness failed\n  -> a vendored source it resolves is not provisioned -- \
                 run: nros setup --source px4-rs"
            ),
            Some("px4-rs")
        );
        assert_eq!(unprovisioned_source("error[E0428]: defined twice"), None);
        assert_eq!(unprovisioned_source("nros setup --source "), None);

        // A marker from an earlier genuine failure at the same key must not
        // survive either: the probe that just ran says nothing about it.
        mark_unprobeable_with_reason(&sidecar, "k", Some(&dir), Some("error: real"));
        assert!(is_known_unprobeable(&sidecar, "k", Some(&dir)));
        mark_unprobeable_with_reason(&sidecar, "k", Some(&dir), Some(log));
        assert!(
            !is_known_unprobeable(&sidecar, "k", Some(&dir)),
            "an unprovisioned source is the host's state, outside the key: \
             a marker would skip the probe after `nros setup --source`"
        );
        assert!(!unprobeable_marker(&sidecar).exists());

        // The negative control: a compile error still records.
        mark_unprobeable_with_reason(&sidecar, "k", Some(&dir), Some("error: real"));
        assert!(is_known_unprobeable(&sidecar, "k", Some(&dir)));
    }

    /// The pick is the ERROR line, and a failure with no error line still
    /// records something rather than nothing.
    #[test]
    fn the_recorded_line_is_the_error_or_the_first_line() {
        assert_eq!(
            first_error_line("a preamble\nerror: static assertion failed: NROS_UNBOUNDED__x\nmore"),
            "error: static assertion failed: NROS_UNBOUNDED__x"
        );
        assert_eq!(
            first_error_line("\n\nmetadata probe run failed for `p::c` (exit 101):\n"),
            "metadata probe run failed for `p::c` (exit 101):"
        );
        assert_eq!(first_error_line(""), "");
        // A link failure keeps the linker's cause, not collect2's summary.
        let link = "metadata probe build failed for `p::c` (exit 2):\n\
            [ 90%] Linking CXX executable probe_p__c\n\
            /usr/bin/ld: CMakeFiles/probe_p__c.dir/probe_p__c.cpp.o: in function `__nros_entry_setup()':\n\
            probe_p__c.cpp:(.text+0x6f): undefined reference to `__nros_c_component_p_create'\n\
            collect2: error: ld returned 1 exit status\n";
        let got = first_error_line(link);
        assert!(
            got.contains("undefined reference to `__nros_c_component_p_create'"),
            "{got}"
        );
        // A CMake error keeps its message, not only its `at <file>:<line>` header.
        let cm = "metadata probe configure failed for `ws` (exit 1):\n\
            CMake Error at /x/cmake/NanoRosPackageXml.cmake:96 (message):\n  \
            nano-ros: project(c_typed_carrier) in /x/CMakeLists.txt\n  \
            does not match the package name `p` in /x/package.xml.\n\
            \n\
            -- Configuring incomplete, errors occurred!\n";
        let got = first_error_line(cm);
        assert!(got.contains("does not match the package name `p`"), "{got}");
        assert!(!got.contains("Configuring incomplete"), "{got}");
        // At real path lengths the 300-character cap still keeps the mismatch.
        let real = "CMake Error at /mnt/renegade/aeon/nros-wt-orph/cmake/NanoRosPackageXml.cmake:96 (message):\n  \
            nano-ros: project(c_typed_carrier) in\n  \
            /mnt/renegade/aeon/nros-wt-orph/examples/rv-virt-threadx/c/typed-carrier/src/riscv64_threadx_c_typed_carrier/CMakeLists.txt\n  \
            does not match the package name `riscv64_threadx_c_typed_carrier` in\n  \
            /mnt/renegade/aeon/nros-wt-orph/examples/rv-virt-threadx/c/typed-carrier/src/riscv64_threadx_c_typed_carrier/package.xml.\n";
        let got = first_error_line(real);
        assert!(got.contains("project(c_typed_carrier)"), "{got}");
        assert!(got.contains("riscv64_threadx_c_typed_carrier"), "{got}");
        // lld's spelling of the same.
        assert!(
            first_error_line("ld.lld: error: undefined symbol: foo\n")
                .contains("undefined symbol: foo")
        );
        let long = format!("error: {}", "x".repeat(400));
        let got = first_error_line(&long);
        assert!(
            got.chars().count() == 301 && got.ends_with('…'),
            "a marker is a cache entry, not a log: got {} chars",
            got.chars().count()
        );
    }

    /// Issue 1898 — the marker keeps WHY a git dependency could not be got.
    /// The harness's own report is one ~300-character line (a package path in
    /// the middle), so cutting at the cap dropped exactly the root cause.
    #[test]
    fn the_recorded_line_keeps_a_cargo_root_cause_past_the_cap() {
        let path = "/w/runner/_work/nano-ros/nros-fixture-tree/packages/core/nros-macros";
        let why = format!(
            "metadata-mode harness failed (exit 101) for component 'reliable_talker': \
             error: failed to get `ros-launch-manifest-model` as a dependency of package \
             `nros-macros v0.6.0 ({path})` (root cause: failed to resolve address for \
             github.com: Temporary failure in name resolution; class=Net (12))"
        );
        assert!(why.chars().count() > 300, "the case must exceed the cap");
        let got = first_error_line(&why);
        assert!(got.chars().count() <= 301, "{got}");
        assert!(
            got.ends_with("Temporary failure in name resolution; class=Net (12))"),
            "{got}"
        );
        // And a raw cargo stream (a C/C++ probe's captured log) gets its cause
        // appended the same way.
        let raw = "error: failed to get `x` as a dependency of package `y`\n\n\
                   Caused by:\n  failed to load source for dependency `x`\n\n\
                   Caused by:\n  revspec 'v9' not found\n";
        assert_eq!(
            first_error_line(raw),
            "error: failed to get `x` as a dependency of package `y` \
             (root cause: revspec 'v9' not found)"
        );
    }

    #[test]
    fn digest_is_stable_and_content_sensitive() {
        let dir = tmp("digest");
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        let first = source_digest(&dir).unwrap();
        assert_eq!(first, source_digest(&dir).unwrap(), "stable across reads");
        std::fs::write(dir.join("src/lib.rs"), "fn b() {}").unwrap();
        assert_ne!(
            first,
            source_digest(&dir).unwrap(),
            "content change moves it"
        );
    }

    /// The whole point of content-addressing: rewriting a file with identical
    /// bytes (what `git pull` / `git stash pop` do to a whole tree) must NOT
    /// invalidate the sidecar.
    #[test]
    fn digest_ignores_mtime_churn() {
        let dir = tmp("mtime");
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        let first = source_digest(&dir).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        assert_eq!(first, source_digest(&dir).unwrap());
    }

    /// Build output must not feed the digest — otherwise every build
    /// invalidates the sidecar that the build just produced.
    #[test]
    fn digest_skips_build_output_and_the_sidecar_dir() {
        let dir = tmp("skip");
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        let before = source_digest(&dir).unwrap();
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();
        std::fs::write(dir.join("target/debug/blob"), "junk").unwrap();
        std::fs::create_dir_all(dir.join("metadata")).unwrap();
        std::fs::write(dir.join("metadata/talker.json"), "{}").unwrap();
        assert_eq!(before, source_digest(&dir).unwrap());
    }

    /// phase-463 W4 -- the number restated at the top of this file is the
    /// recorder's own. Read from the recorder's source rather than trusted,
    /// because the two live in workspaces that share no crate.
    #[test]
    fn the_recorder_schema_version_matches_the_recorder() {
        let recorder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../api/nros/src/node_metadata.rs");
        // Issue 1552 — this crate is built from the checkout, never packaged, so
        // the sibling is always there; its absence is a moved file, and a test
        // that returned here PASSED having compared nothing.
        let text = std::fs::read_to_string(&recorder).unwrap_or_else(|e| {
            panic!(
                "{}: {e} — the recorder moved; update this path",
                recorder.display()
            )
        });
        let declared = text
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("pub const SOURCE_METADATA_SCHEMA_VERSION: u32 = ")?
                    .strip_suffix(';')?
                    .trim()
                    .parse::<u32>()
                    .ok()
            })
            .expect("the recorder declares its schema version");
        assert_eq!(
            declared, RECORDER_SCHEMA_VERSION,
            "the recorder moved to schema {declared}; every census written under \
             {RECORDER_SCHEMA_VERSION} is stale by definition and this constant has to say so"
        );
    }

    /// A fake nano-ros root: a closure list naming `packages/core/probed`, and
    /// that crate plus one the list does NOT name.
    fn fake_nano_ros(name: &str) -> PathBuf {
        let root = tmp(name);
        let list = root.join(PROBE_SOURCE_DIRS);
        std::fs::create_dir_all(list.parent().unwrap()).unwrap();
        std::fs::write(&list, "# generated\npackages/core/probed\n").unwrap();
        for krate in ["probed", "unprobed"] {
            let src = root.join("packages/core").join(krate).join("src");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(src.join("lib.rs"), "pub const IN_PLACE: bool = false;\n").unwrap();
        }
        root
    }

    /// Issue 1578 — THE reproduction, at the level the key decides it. A change
    /// to a crate the probe COMPILES must change the key while the component's
    /// own sources stay byte-identical. Before this, the key was the component's
    /// digest alone, so the sidecar read "already current" and a stale
    /// `in_place` was reused.
    #[test]
    fn a_change_to_a_crate_the_probe_compiles_changes_the_key() {
        let root = fake_nano_ros("probe-key-root");
        let pkg = tmp("probe-key-pkg");
        std::fs::write(pkg.join("src/lib.rs"), "// the component\n").unwrap();

        let before = probe_inputs_key(&pkg, Some(&root)).expect("keyed");
        // The component is UNTOUCHED; only the probed crate moves.
        std::fs::write(
            root.join("packages/core/probed/src/lib.rs"),
            "pub const IN_PLACE: bool = true;\n",
        )
        .unwrap();
        let after = probe_inputs_key(&pkg, Some(&root)).expect("keyed");
        assert_ne!(
            before, after,
            "a change to a crate the probe compiles must re-probe -- this is the \
             `in_place` flip #1578 measured being reused"
        );
    }

    // Issue 2640 -- the launch values a component is probed with are part of
    // its key, so editing a parameter file re-probes; a component no launch
    // names keeps the key it had before seeds existed.
    #[test]
    fn the_launch_values_a_component_is_probed_with_are_in_its_key() {
        let root = fake_nano_ros("probe-key-seeds");
        let pkg = tmp("probe-key-seeds-pkg");
        std::fs::write(pkg.join("src/lib.rs"), "// the component\n").unwrap();
        let base = probe_inputs_key(&pkg, Some(&root)).expect("keyed");
        assert_eq!(
            component_probe_key(&pkg, Some(&root), &Seeding::None),
            Some(base.clone())
        );
        let seeded = |v: &str| Seeding::Seeded {
            instances: vec!["/c".into()],
            params: vec![("ctrl_period_ms".into(), v.into())],
            sources: Vec::new(),
            digest: format!("sha256:{v}"),
        };
        let a = component_probe_key(&pkg, Some(&root), &seeded("20")).expect("keyed");
        let b = component_probe_key(&pkg, Some(&root), &seeded("25")).expect("keyed");
        assert!(a.starts_with(&base) && a != base, "{a}");
        assert_ne!(a, b, "another launch value must re-probe");
    }

    /// The other direction: a crate the probe does NOT compile must not re-probe
    /// every component. Over-watch is the safe side, but an unlisted crate is
    /// not over-watch, it is noise -- and the list is what makes the difference
    /// measurable.
    #[test]
    fn a_change_to_a_crate_the_probe_does_not_compile_keeps_the_key() {
        let root = fake_nano_ros("probe-key-unlisted");
        let pkg = tmp("probe-key-unlisted-pkg");
        std::fs::write(pkg.join("src/lib.rs"), "// the component\n").unwrap();

        let before = probe_inputs_key(&pkg, Some(&root)).expect("keyed");
        std::fs::write(
            root.join("packages/core/unprobed/src/lib.rs"),
            "pub const IN_PLACE: bool = true;\n",
        )
        .unwrap();
        assert_eq!(before, probe_inputs_key(&pkg, Some(&root)).expect("keyed"));

        // ...and the component's own change still moves it, as it always did.
        std::fs::write(pkg.join("src/lib.rs"), "// the component, edited\n").unwrap();
        assert_ne!(before, probe_inputs_key(&pkg, Some(&root)).expect("keyed"));
    }

    /// No list, or no nano-ros path, is NEVER fresh -- never a key over a
    /// smaller closure. `cli-source-dirs.txt`'s rule for the CLI stamp.
    #[test]
    fn a_missing_closure_list_is_never_fresh() {
        let pkg = tmp("probe-key-nolist-pkg");
        std::fs::write(pkg.join("src/lib.rs"), "// c\n").unwrap();
        let bare = tmp("probe-key-nolist-root");
        assert_eq!(probe_inputs_key(&pkg, Some(&bare)), None, "no list");
        assert_eq!(probe_inputs_key(&pkg, None), None, "no nano-ros path");

        // An EMPTY list is a broken list, not "nothing to watch".
        let list = bare.join(PROBE_SOURCE_DIRS);
        std::fs::create_dir_all(list.parent().unwrap()).unwrap();
        std::fs::write(&list, "# only a header\n").unwrap();
        assert_eq!(probe_inputs_key(&pkg, Some(&bare)), None, "empty list");

        // And the sentinel a keyless sidecar is stamped with can never read as
        // fresh against any key this function returns.
        let root = fake_nano_ros("probe-key-sentinel");
        let key = probe_inputs_key(&pkg, Some(&root)).expect("keyed");
        assert_ne!(key, UNKEYED);
    }

    /// phase-463 W4 -- the four answers `recompute_digest` can give, and the
    /// one that matters: TOUCHING a source file is not a change.
    #[test]
    fn a_recorded_source_tree_is_stale_by_content_and_not_by_mtime() {
        let dir = tmp("census-fresh");
        let pkg = dir.join("src/comp");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("node.cpp"), "// SUB: /a\n").unwrap();
        let recorded = source_digest(&pkg).unwrap();
        let inputs = vec![RecordedInput {
            role: "source_tree".into(),
            path: "src/comp".into(),
            digest: recorded.clone(),
        }];

        assert!(
            stale_recorded_inputs(&dir, &inputs).is_empty(),
            "nothing changed"
        );

        // TOUCH: same bytes, new mtime. This is the step the gate's fourth
        // move exercises, and the whole reason staleness is content-addressed.
        std::fs::write(pkg.join("node.cpp"), "// SUB: /a\n").unwrap();
        assert!(
            stale_recorded_inputs(&dir, &inputs).is_empty(),
            "a touch is not a change"
        );

        // A real edit -- one more entity the code creates.
        std::fs::write(pkg.join("node.cpp"), "// SUB: /a\n// SUB: /b\n").unwrap();
        let stale = stale_recorded_inputs(&dir, &inputs);
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert_eq!(stale[0].role, "source_tree");
        assert!(stale[0].line().contains("src/comp"), "{}", stale[0].line());
        assert!(
            stale[0].line().contains(&recorded),
            "the refusal names the digest it expected: {}",
            stale[0].line()
        );
    }

    /// A recorded input that is GONE is stale, never skipped: "I could not
    /// check" must not read as "it still matches".
    #[test]
    fn an_input_that_cannot_be_hashed_is_stale_rather_than_skipped() {
        let dir = tmp("census-gone");
        let inputs = vec![
            RecordedInput {
                role: "binary".into(),
                path: "build/entry".into(),
                digest: "sha256:0".into(),
            },
            RecordedInput {
                role: "source_tree".into(),
                path: "src/vanished".into(),
                digest: "fnv1a64:0".into(),
            },
            RecordedInput {
                role: "mystery".into(),
                path: "x".into(),
                digest: "crc32:0".into(),
            },
        ];
        let stale = stale_recorded_inputs(&dir, &inputs);
        assert_eq!(stale.len(), 3, "{stale:?}");
        assert!(stale[0].current.contains("cannot read"), "{stale:?}");
        assert!(stale[1].current.contains("not a directory"), "{stale:?}");
        assert!(
            stale[2].current.contains("unknown digest algorithm"),
            "{stale:?}"
        );
    }

    /// phase-517 W5 (RFC-0100 Amendment 3) -- end to end over a real
    /// workspace: a contract-less model whose launched node has a FRESH
    /// sidecar is stated by it (`StatedBy::Probe`); the same sidecar gone
    /// stale against its sources leaves the image unstated, as before.
    #[test]
    fn a_fresh_sidecar_states_a_contract_less_image_and_a_stale_one_does_not() {
        use crate::entity_inventory::StatedBy;
        let root = fake_nano_ros("probe-inventory-root");
        let ws = tmp("probe-inventory-ws");
        let pkg = ws.join("src/talker_pkg");
        std::fs::create_dir_all(pkg.join("src")).unwrap();
        std::fs::create_dir_all(pkg.join("metadata")).unwrap();
        std::fs::write(
            pkg.join("package.xml"),
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        )
        .unwrap();
        std::fs::write(
            pkg.join("nros.toml"),
            "[component]\nversion = 1\npackage = \"talker_pkg\"\ncomponent = \"talker\"\n\
             language = \"rust\"\n[component.metadata]\nsource_metadata = \"metadata/talker.json\"\n",
        )
        .unwrap();
        std::fs::write(pkg.join("src/lib.rs"), "// the component\n").unwrap();
        let sidecar = pkg.join("metadata/talker.json");
        std::fs::write(
            &sidecar,
            r#"{"version": 4, "package": "talker_pkg", "component": "talker",
                "language": "rust", "executable": "talker", "exported_symbol": null,
                "nodes": [{"id": "talker", "declaration_slot": 0,
                    "source_default_name": "talker",
                    "unresolved_name": {"value": "talker", "kind": "relative"},
                    "namespace": null,
                    "publishers": [{"id": "/chatter", "declaration_slot": 0,
                        "unresolved_topic": {"value": "/chatter", "kind": "absolute"},
                        "interface": {"package": "std_msgs", "name": "msg/Int32",
                                      "kind": "message"},
                        "qos": {"reliability": "reliable", "durability": "volatile",
                                "history": "keep_last", "depth": 10,
                                "deadline_ms": null, "lifespan_ms": null,
                                "liveliness": "system_default",
                                "liveliness_lease_duration_ms": null,
                                "extensions": {}}}],
                    "subscribers": [],
                    "timers": [{"id": "on_tick", "declaration_slot": 1,
                                "period_ms": 1000, "period_us": 1000000, "kind": "wall",
                                "callback": "on_tick", "callback_slot": 0}],
                    "services": [], "actions": []}],
                "callbacks": [], "parameters": [],
                "trace": {"generator": "t", "package_manifest": "package.xml",
                          "source_artifacts": []}}"#,
        )
        .unwrap();
        let key = probe_inputs_key(&pkg, Some(&root)).expect("keyed");
        stamp_provenance(&sidecar, &key).unwrap();

        let model = crate::entity_inventory::bare_without_stock(
            serde_yaml_ng::from_str(
                "meta: { version: 1 }\nstructure:\n  nodes:\n    /talker:\n      \
                 { scope: system.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }\n",
            )
            .unwrap(),
        );

        let (inv, read, notes, stale) = probe_inventory_with("m", &ws, Some(&root), &model);
        assert!(stale.is_empty(), "{stale:?}");
        let inv = inv.unwrap_or_else(|| panic!("a fresh sidecar states the image: {notes:?}"));
        assert_eq!(inv.stated_by(), StatedBy::Probe);
        assert_eq!(read, vec![sidecar.clone()]);
        let d = inv.derive();
        let k = d.knobs().expect("derives");
        assert_eq!((k.max_cbs, k.max_publishers), (1, 1), "{k:?}");

        // The component's source moves; the sidecar is now stale, so the probe
        // states nothing -- and, issue 2734, the verdict names the stale
        // launched component as a failure, never "nothing declared".
        std::fs::write(pkg.join("src/lib.rs"), "// the component, edited\n").unwrap();
        let (inv, _, notes, stale) = probe_inventory_with("m", &ws, Some(&root), &model);
        assert!(inv.is_none(), "a stale sidecar states nothing");
        assert!(
            notes.iter().any(|n| n.contains("stale")),
            "the skip is said: {notes:?}"
        );
        assert_eq!(
            stale.iter().map(|s| s.pkg.as_str()).collect::<Vec<_>>(),
            ["talker_pkg"]
        );
        let why = stale_launched_reason(&model, &stale).expect("a launched package is stale");
        assert!(
            why.contains("1 launched component(s) are stale") && why.contains("Moved:"),
            "{why}"
        );
        // A stale sidecar of a package this image does not launch is not its
        // failure.
        let other = nros_orchestration_ir::overlay::LoadedModel::bare(
            serde_yaml_ng::from_str(
                "meta: { version: 1 }\nstructure:\n  nodes:\n    /other:\n      \
                 { scope: system.launch.xml, pkg: other_pkg, exec: other, node_name: other }\n",
            )
            .unwrap(),
        );
        assert_eq!(stale_launched_reason(&other, &stale), None);
    }

    #[test]
    fn a_missing_or_unstamped_sidecar_is_stale() {
        let dir = tmp("fresh");
        let sidecar = dir.join("metadata.json");
        assert!(!sidecar_is_fresh(&sidecar, "fnv1a64:0"), "missing ⇒ stale");
        std::fs::write(&sidecar, "not json").unwrap();
        assert!(!sidecar_is_fresh(&sidecar, "fnv1a64:0"), "garbage ⇒ stale");
    }

    /// A sidecar body as the probe leaves it: `stamped` adds the provenance
    /// [`stamp_provenance`] writes on every successful probe.
    /// Issue 2689 -- a probe sidecar this CLI's `SourceMetadata` cannot read
    /// fails the stamp LOUDLY, and the message names the key. The C/C++ batch
    /// used to drop this error (`let _ =`), which left every v6 sidecar
    /// unstamped and so stale, with nothing said.
    #[test]
    fn a_sidecar_the_mirror_cannot_read_fails_the_stamp_naming_the_key() {
        let dir = tmp("stamp-unknown-key");
        let sidecar = dir.join("talker.json");
        let body = sidecar_body("demo_pkg::talker", false).replacen(
            "\"nodes\"",
            "\"a_key_from_the_future\": 1, \"nodes\"",
            1,
        );
        std::fs::write(&sidecar, &body).unwrap();
        let why = format!(
            "{}",
            stamp_provenance(&sidecar, "fnv1a64:0").expect_err("an unknown key is refused")
        );
        assert!(
            why.contains("a_key_from_the_future"),
            "the error must name the key, got: {why}"
        );
        // Untouched: no stamp lands on a document the mirror rejected.
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), body);
    }

    fn sidecar_body(component: &str, stamped: bool) -> String {
        let provenance = if stamped {
            r#", "provenance": {"inputs_digest": "fnv1a64:0", "generator": "nros test"}"#
        } else {
            ""
        };
        format!(
            r#"{{"version": 4, "package": "demo_pkg", "component": "{component}",
                "language": "rust", "executable": null, "exported_symbol": null,
                "nodes": [], "callbacks": [], "parameters": [],
                "trace": {{"generator": "t", "package_manifest": "package.xml",
                           "source_artifacts": []}}{provenance}}}"#
        )
    }

    /// Issue 1639 — a renamed component's old sidecar is REMOVED, so no reader
    /// that globs the directory can compose it into the image again.
    ///
    /// The declared one stays, a hand-written (unstamped) one stays and is
    /// REPORTED, and the old one's negative-cache marker goes with it, as does a
    /// marker whose component is gone. The leaf reader is asserted directly,
    /// because it is the one that sized issue 1639's 162,936-byte arena.
    #[test]
    fn a_renamed_components_old_sidecar_is_pruned() {
        let dir = tmp("prune");
        let md = dir.join("metadata");
        std::fs::create_dir_all(&md).unwrap();
        // `probeable_decl` declares `metadata/talker.json`.
        let decl = probeable_decl(&dir, ComponentLanguage::Rust);
        std::fs::write(md.join("talker.json"), sidecar_body("talker", true)).unwrap();
        std::fs::write(md.join("old_talker.json"), sidecar_body("old_talker", true)).unwrap();
        std::fs::write(md.join("old_talker.json.unprobeable"), "k").unwrap();
        std::fs::write(md.join("gone.json.unprobeable"), "k").unwrap();
        std::fs::write(md.join("hand.json"), sidecar_body("hand", false)).unwrap();
        std::fs::write(md.join("notes.txt"), "not a sidecar").unwrap();

        let (before, _) = crate::leaf_entity_env::inventory_for_leaf(&dir).unwrap();
        assert_eq!(
            before.len(),
            3,
            "precondition: the leaf reader globs all three sidecars"
        );

        let mut report = RefreshReport::default();
        prune_undeclared_sidecars(std::slice::from_ref(&decl), &mut report);

        assert_eq!(report.pruned, vec![md.join("old_talker.json")]);
        assert_eq!(report.undeclared_kept, vec![md.join("hand.json")]);
        assert!(
            md.join("talker.json").exists(),
            "the declared sidecar stays"
        );
        assert!(
            md.join("hand.json").exists(),
            "an unstamped file is not ours"
        );
        assert!(md.join("notes.txt").exists());
        assert!(!md.join("old_talker.json").exists());
        assert!(!md.join("old_talker.json.unprobeable").exists());
        assert!(!md.join("gone.json.unprobeable").exists());

        let (after, _) = crate::leaf_entity_env::inventory_for_leaf(&dir).unwrap();
        assert_eq!(
            after.len(),
            2,
            "the renamed-away component is no longer read"
        );

        // Idempotent: a second pass finds nothing more to remove.
        let mut again = RefreshReport::default();
        prune_undeclared_sidecars(std::slice::from_ref(&decl), &mut again);
        assert!(again.pruned.is_empty());
    }
}
