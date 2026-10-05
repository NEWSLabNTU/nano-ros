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
    /// Issue 1639 — sidecars the probe wrote for a component no declaration
    /// names any more (a rename, a deletion), removed this pass.
    pub pruned: Vec<PathBuf>,
    /// Issue 1639 — `*.json` files in a sidecar directory that name no
    /// declared component and carry no probe provenance, so this pass did not
    /// write them and will not delete them. Every reader that globs the
    /// directory still sees them, so they are REPORTED.
    pub undeclared_kept: Vec<PathBuf>,
}

impl RefreshReport {
    pub fn total(&self) -> usize {
        self.rebuilt.len() + self.fresh.len()
    }
}

/// Refresh every stale source-metadata sidecar in the workspace.
///
/// `nano_ros` is the nano-ros checkout the harness path-depends on for `nros`;
/// without it no harness can be compiled and the whole step is skipped (a
/// user's workspace built against an installed SDK has no such path).
pub fn refresh_stale_sidecars(
    ws_root: &Path,
    nano_ros: Option<&Path>,
    verbose: bool,
) -> Result<RefreshReport> {
    let mut report = RefreshReport::default();
    let workspace = Workspace::discover(ws_root)?;
    let declarations = workspace.component_declarations()?;
    if declarations.is_empty() {
        return Ok(report);
    }
    // Issue 1639 — before anything is probed, so a stale sidecar can never be
    // read by this pass either.
    prune_undeclared_sidecars(&declarations, &mut report);
    let probe_root = ws_root.join("build").join("nros-metadata");
    // Issue 0286 — resolved once, not per component.
    let host = host_triple();
    // phase-313 — C/C++ components to probe in ONE workspace-scoped project.
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
            match cpp_probe_options(decl, nano_ros, &probe_root) {
                // issue 0641 — a C/C++ probe that FAILED at this exact source is
                // not re-attempted until something changes. The Rust branch
                // below has had this since #0288; the batch path never did, so
                // an unprobeable component paid a full CMake configure + build
                // on EVERY sync, for ever. Measured on
                // `examples/workspaces/features`, whose probe project fails to
                // configure: 17 components, 1.2 s of cmake per sync, no
                // possible progress. `nros sync` is run 22 times by
                // `regenerate-bindings.sh` at the head of every fixture build.
                Ok(Some(opts)) if is_known_unprobeable_now(&opts.output_path, decl, nano_ros) => {
                    // issue 1469 — carry the RECORDED CAUSE up, not just the
                    // fact of the skip. "probe failed at this source last sync;
                    // unchanged" named the node and nothing else, and because
                    // the skip never re-attempts, the error it stands for is
                    // reachable from no later sync at all. The marker holds the
                    // first error line; say it here, and where the whole file
                    // is, so the next reader starts at the cause.
                    let why = match recorded_unprobeable_reason(&opts.output_path) {
                        Some(line) => format!("; last error: {line}"),
                        None => String::new(),
                    };
                    report.unsupported.push(format!(
                        "{}::{} (probe failed at this source last sync; unchanged{why})",
                        decl.config.package, decl.config.component
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
                Ok(Some(opts)) => cpp_batch.push(opts),
                // Already current — no probe needed.
                Ok(None) => report.fresh.push(decl.source_metadata_path()),
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
            report.unsupported.push(format!(
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
        match build_metadata(&build_options(decl, nano_ros, &probe_root)) {
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
            Err(why) if decl.deploy_bound => {
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
                report.unsupported.push(format!(
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
        match crate::orchestration::metadata_probe_cmake::run_probes(&dir, &cpp_batch) {
            Ok(outcomes) => {
                for (o, opts) in outcomes.iter().zip(cpp_batch.iter()) {
                    match &o.result {
                        Ok(()) => {
                            // Stamp only on success; a failed probe leaves the
                            // previous sidecar (or none) untouched.
                            let key =
                                probe_inputs_key(&opts.package_dir, Some(&opts.nano_ros_workspace));
                            let _ = stamp_provenance(
                                &opts.output_path,
                                key.as_deref().unwrap_or(UNKEYED),
                            );
                            report.rebuilt.push(opts.output_path.clone());
                        }
                        Err(why) => {
                            let text = format!("{why:#}");
                            mark_unprobeable_now(
                                &opts.output_path,
                                &opts.package_dir,
                                nano_ros,
                                compile_root().as_deref(),
                                &text,
                            );
                            report
                                .unsupported
                                .push(format!("{}::{} ({text})", o.package, o.component));
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
                    report
                        .unsupported
                        .push(format!("{}::{} ({why})", opts.package, opts.component));
                }
            }
            // A configure failure is the whole project, not one component.
            Err(why) => {
                let text = format!("probe project configure failed: {why:#}");
                for opts in &cpp_batch {
                    // Every component in the batch shares the project that
                    // failed to configure, so every one of them is unprobeable
                    // until something changes — mark each, or the next sync
                    // rebuilds the same broken project.
                    mark_unprobeable_now(
                        &opts.output_path,
                        &opts.package_dir,
                        nano_ros,
                        compile_root().as_deref(),
                        &text,
                    );
                    report
                        .unsupported
                        .push(format!("{}::{} ({text})", opts.package, opts.component));
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
) -> Result<Option<crate::orchestration::metadata_probe_cmake::CmakeProbeOptions>, String> {
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
    // Issue 1578 — the same key as the Rust branch; `nano_ros` is already known
    // present here (the `let Some` above), so a `None` means only that the
    // closure list is missing, which is never fresh.
    if probe_inputs_key(&decl.package_root, Some(nano_ros))
        .is_some_and(|k| sidecar_is_fresh(&sidecar, &k))
    {
        return Ok(None);
    }

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
    Ok(Some(
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
        },
    ))
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
/// Returns the inventory, the sidecar paths it READ (configure-time inputs —
/// issue 1018), and one note per skip.
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
) -> (
    crate::entity_inventory::EntityInventory,
    Vec<PathBuf>,
    Vec<String>,
) {
    let nano_ros = crate::orchestration::nano_ros_root::resolve(None, ws_root);
    let (rows, read, mut notes) = match fresh_probe_rows(ws_root, nano_ros.as_deref()) {
        Ok(v) => v,
        Err(e) => (
            Vec::new(),
            Vec::new(),
            vec![format!(
                "no probe sidecar read ({e}); every launched node the contract does not \
                 describe refuses the derivation"
            )],
        ),
    };
    let recorded = crate::entity_inventory::EntityInventory::recorded_for_launch(model, &rows);
    notes.dedup();
    (recorded.merged_per_kind_max(contract), read, notes)
}

/// Issue 1694 -- THE contract inventory every model road composes: what
/// [`crate::entity_inventory::EntityInventory::from_model`] makes of `model`,
/// floored by [`floor_by_recorded`] when the road knows its workspace.
///
/// `None` exactly when `from_model` is `None` (no contract authored), so the
/// "no contract, no change" control every caller guards on is untouched.
/// `who` prefixes the notes this prints.
pub fn contract_inventory(
    source: impl Into<String>,
    model: &ros_launch_manifest_model::SystemModel,
    workspace: Option<&Path>,
    who: &str,
) -> Option<crate::entity_inventory::EntityInventory> {
    let contract = crate::entity_inventory::EntityInventory::from_model(source, model)?;
    let Some(ws) = workspace else {
        return Some(contract);
    };
    let (floored, _read, notes) = floor_by_recorded(ws, model, &contract);
    for n in notes {
        eprintln!("{who}: {n}");
    }
    Some(floored)
}

/// One FRESH probe sidecar, as [`fresh_probe_rows`] read it.
#[derive(Debug, Clone)]
pub struct ProbeRow {
    /// The sidecar's `executable` -- with `entities.pkg`, the
    /// `(package, executable)` key a launch-tree node (`pkg` + `exec`) names
    /// it by, which is the key `sidecar_slots::slots_of_component` counts on.
    pub executable: Option<String>,
    pub entities: crate::entity_inventory::ComponentEntities,
}

/// The workspace's FRESH probe sidecars, one row each, under the freshness rule
/// [`fresh_probe_inventory`] documents -- that function and issue 1694's
/// count floor ([`crate::entity_inventory::EntityInventory::recorded_for_launch`])
/// read ONE set of sidecars through this.
pub fn fresh_probe_rows(
    ws_root: &Path,
    nano_ros: Option<&Path>,
) -> Result<(Vec<ProbeRow>, Vec<PathBuf>, Vec<String>)> {
    use crate::entity_inventory::ComponentEntities;

    let workspace = Workspace::discover(ws_root)?;
    let mut notes = Vec::new();
    let mut read: Vec<PathBuf> = Vec::new();
    let mut rows: Vec<ProbeRow> = Vec::new();
    for decl in workspace.component_declarations()? {
        let sidecar = decl.source_metadata_path();
        if !sidecar.is_file() {
            continue;
        }
        let fresh = probe_inputs_key(&decl.package_root, nano_ros)
            .is_some_and(|k| sidecar_is_fresh(&sidecar, &k));
        if !fresh {
            notes.push(format!(
                "{}: stale against its sources (re-run `nros sync`), so neither its counts nor \
                 its registration observations are used",
                sidecar.display()
            ));
            continue;
        }
        let raw = std::fs::read_to_string(&sidecar)
            .wrap_err_with(|| format!("read {}", sidecar.display()))?;
        match crate::leaf_entity_env::declaration_from_probe(&raw) {
            Ok((pkg, component, declaration)) => {
                let executable = serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|v| v.get("executable")?.as_str().map(str::to_string));
                read.push(sidecar);
                rows.push(ProbeRow {
                    executable,
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
    Ok((rows, read, notes))
}

/// Issue 1639 — remove the sidecars of components no declaration names.
///
/// The probe writes one `<dir>/<component>.json` per DECLARED component, and
/// every reader of that directory — `leaf_entity_env::inventory_for_leaf`
/// (the cargo-leaf sizing descriptor), `Workspace::source_metadata_files`
/// (the planner, model ingest, `nros metadata`) — globs `*.json` in it. So a
/// rename left the old component's sidecar behind and the next sync composed
/// it into the image: measured on `bins/in-place-subscriptions` after
/// `five_listener` → `eight_listener`, 13 subscription rows for an image
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
fn stamp_provenance(sidecar: &Path, digest: &str) -> Result<()> {
    let raw =
        std::fs::read_to_string(sidecar).wrap_err_with(|| format!("read {}", sidecar.display()))?;
    let mut parsed: SourceMetadata = serde_json::from_str(&raw).wrap_err_with(|| {
        format!(
            "metadata harness emitted invalid JSON at {}",
            sidecar.display()
        )
    })?;
    parsed.provenance = Some(SourceMetadataProvenance {
        inputs_digest: digest.to_string(),
        generator: format!("nros {}", env!("CARGO_PKG_VERSION")),
    });
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
/// Issue 1578 retired `unprobeable_key` (the package digest plus the CLI stamp,
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
) -> bool {
    probe_inputs_key(&decl.package_root, nano_ros)
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
    package_dir: &Path,
    nano_ros: Option<&Path>,
    compile_root: Option<&Path>,
    why: &str,
) {
    if let Some(key) = probe_inputs_key(package_dir, nano_ros) {
        mark_unprobeable_with_reason(sidecar, &key, compile_root.or(nano_ros), Some(why));
    }
}

/// What a sidecar is stamped with when no key could be formed. It never equals
/// a key [`probe_inputs_key`] returns (those are `fnv1a64:...` triples), so a
/// sidecar stamped with it is re-probed on the next sync — the "never fresh"
/// half of `None`.
const UNKEYED: &str = "unkeyed";

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
fn first_error_line(why: &str) -> String {
    const CAP: usize = 300;
    let pick = why
        .lines()
        .map(str::trim)
        .find(|l| {
            let lower = l.to_ascii_lowercase();
            lower.starts_with("error") || lower.contains("error:") || lower.contains("error[")
        })
        .or_else(|| why.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("");
    let mut line: String = pick.chars().take(CAP).collect();
    if pick.chars().count() > CAP {
        line.push('…');
    }
    line
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
/// 2. the CLI that wrote the harness — `NROS_CLI_SOURCE_STAMP`, baked from the
///    CLI's own sources by `build.rs`, because `CARGO_PKG_VERSION` (the only
///    CLI input `source_digest` mixes) moves on a release and not on a fix;
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
    let pkg = source_digest(package_root).ok()?;
    let closure = probe_closure_digest(nano_ros?)?;
    Some(format!("{pkg}:{}:{closure}", env!("NROS_CLI_SOURCE_STAMP")))
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
pub const RECORDER_SCHEMA_VERSION: u32 = 4;

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
    let raw = std::fs::read_to_string(system_toml)
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
        assert_eq!(none, "capabilities:");

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
        assert_eq!(listed, "capabilities:param_services,lifecycle");
        std::fs::write(&st, format!("{base}\n[param_services]\nenabled = true\n")).unwrap();
        assert_eq!(
            capabilities_digest(&st).unwrap(),
            "capabilities:param_services"
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
            stale[0].line().contains("now capabilities:param_services"),
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
            )
            .unwrap_or_else(|why| panic!("{language:?} must be probeable: {why}"))
            .unwrap_or_else(|| panic!("{language:?}: no sidecar exists, so a probe is needed"));
            assert_eq!(opts.language, language, "{language:?}");
        }

        let why = cpp_probe_options(
            &probeable_decl(&dir, ComponentLanguage::Rust),
            Some(&nano_ros),
            &probe_root,
        )
        .expect_err("a Rust component has no cmake probe — it has the cargo harness");
        assert!(
            why.contains("cargo metadata harness"),
            "the refusal must say where a Rust component is produced instead: {why}"
        );
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
        let long = format!("error: {}", "x".repeat(400));
        let got = first_error_line(&long);
        assert!(
            got.chars().count() == 301 && got.ends_with('…'),
            "a marker is a cache entry, not a log: got {} chars",
            got.chars().count()
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
