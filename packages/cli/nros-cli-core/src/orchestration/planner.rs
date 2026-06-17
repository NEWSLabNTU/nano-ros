//! Draft host planner for Phase 126.C.

use super::{
    manifest::{ManifestArtifact, endpoint_requirements, load_manifest},
    names,
    params::{ParameterInputs, effective_parameters, load_toml_values},
    plan::{NrosPlan, PlanBuildOptions, PlanEntity},
    schema::InterfaceRef,
    workspace::{Workspace, unique_paths},
};
use eyre::{Context, Result, bail, eyre};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub system_pkg: String,
    pub workspace_root: PathBuf,
    pub launch_file: PathBuf,
    pub record_file: Option<PathBuf>,
    pub out_root: PathBuf,
    pub metadata_files: Vec<PathBuf>,
    pub manifest_files: Vec<PathBuf>,
    pub nros_toml_files: Vec<PathBuf>,
    pub launch_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PlanningOutput {
    pub record_path: PathBuf,
    pub plan_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CheckReport {
    pub errors: usize,
    pub warnings: usize,
    /// Phase 172 WP-B — the warning messages (len == `warnings`).
    pub messages: Vec<String>,
}

#[derive(Debug, Clone)]
struct JsonArtifact {
    path: PathBuf,
    value: Value,
}

pub fn plan_system(options: PlanOptions) -> Result<PlanningOutput> {
    fs::create_dir_all(&options.out_root)?;
    let metadata_dir = options.out_root.join("metadata");
    fs::create_dir_all(&metadata_dir)?;

    let workspace = Workspace::discover(&options.workspace_root)?;
    let launch_args = parse_launch_args(&options.launch_args)?;
    let record = load_or_parse_record(
        &options.launch_file,
        &options.workspace_root,
        options.record_file.as_deref(),
        launch_args,
    )?;

    let record_path = options.out_root.join("record.json");
    fs::write(&record_path, serde_json::to_string_pretty(&record)?)?;

    let metadata_paths = metadata_paths(&options, &workspace, &metadata_dir);
    let mut metadata = load_json_artifacts(&metadata_paths, "source metadata")?;
    // Phase 212.M-F.17 — α-bridge: synthesise minimal metadata artifacts from
    // workspace-member `Cargo.toml` `[package.metadata.nros.{component,
    // components,node,nodes}]` tables. Appended AFTER the sidecar JSON
    // artifacts so the file artifacts win the `(package, component)` dedup
    // in `schema_components` (back-compat: a package shipping both an
    // authoritative metadata JSON and a stub component table keeps the
    // file's richer data on the plan).
    for (path, value) in workspace.synthetic_metadata_artifacts() {
        metadata.push(JsonArtifact { path, value });
    }
    preserve_metadata(&metadata, &metadata_dir)?;

    let manifest_paths = if options.manifest_files.is_empty() {
        workspace.manifest_files()
    } else {
        unique_paths(options.manifest_files.clone())
    };
    let manifests = manifest_paths
        .iter()
        .map(|path| load_manifest(path))
        .collect::<Result<Vec<_>>>()?;

    let mut nros_toml = options.nros_toml_files.clone();
    if let Some(system_toml) = workspace.package_nros_toml(&options.system_pkg) {
        nros_toml.push(system_toml);
    }
    // Phase 254 — capture the bringup's typed `system.toml` path here (workspace in
    // scope) so the capability axes can read the SSoT later.
    let system_toml_path = workspace.package_system_toml(&options.system_pkg);
    let overlays = load_toml_values(&unique_paths(nros_toml))?;

    let (instances, executables, mut diagnostics) =
        build_instances(&record, &metadata, &workspace, &overlays, &record_path);
    diagnostics.extend(check_effective_graph_node_names(&instances, &record_path));
    diagnostics.extend(check_manifest_endpoints(
        &instances,
        &manifests,
        &metadata,
        &record_path,
    ));

    if diagnostics
        .iter()
        .any(|diag| diag.get("severity").and_then(Value::as_str) == Some("error"))
    {
        return Err(eyre!(
            "planning failed with {} error(s): {}",
            diagnostics
                .iter()
                .filter(|diag| diag.get("severity").and_then(Value::as_str) == Some("error"))
                .count(),
            diagnostics
                .iter()
                .filter(|diag| diag.get("severity").and_then(Value::as_str) == Some("error"))
                .map(diagnostic_summary)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    // Phase 173.5 — derive the `build` block (board / target / rmw /
    // profile / `[[transport]]`) from the nros.toml overlays, then
    // validate the transport semantics with a clear error before the
    // plan is written.
    let build_json = schema_build_json(&overlays, system_toml_path.as_deref());
    let build: PlanBuildOptions = serde_json::from_value(build_json.clone())
        .wrap_err("invalid [build] / [[transport]] section in nros.toml")?;
    let transport_problems = build.validate_transports();
    if !transport_problems.is_empty() {
        return Err(eyre!(
            "invalid [[transport]] config in nros.toml: {}",
            transport_problems.join("; ")
        ));
    }

    let plan = schema_plan_json(
        &options,
        &record_path,
        &instances,
        &executables,
        &metadata,
        &overlays,
        system_toml_path.as_deref(),
        build_json,
    );

    let plan_path = options.out_root.join("nros-plan.json");
    fs::write(&plan_path, serde_json::to_string_pretty(&plan)?)?;
    Ok(PlanningOutput {
        record_path,
        plan_path,
    })
}

pub fn check_plan_file(path: &Path) -> Result<CheckReport> {
    let raw = fs::read_to_string(path)
        .wrap_err_with(|| format!("failed to read plan {}", path.display()))?;
    let plan: NrosPlan = serde_json::from_str(&raw)
        .wrap_err_with(|| format!("invalid nros-plan.json schema {}", path.display()))?;
    let errors = validate_plan(&plan);
    if !errors.is_empty() {
        return Err(eyre!(
            "invalid nros-plan.json graph {}: {} error(s): {}",
            path.display(),
            errors.len(),
            errors.join("; ")
        ));
    }
    let messages = collect_plan_warnings(&plan);
    Ok(CheckReport {
        errors: 0,
        warnings: messages.len(),
        messages,
    })
}

/// Phase 172 WP-B (slice 4) — non-fatal plan warnings. Today: the in-binary
/// RMW-set feasibility check. A bridge that links more than one RMW backend
/// into a single binary (`build.rmw` is effectively a *set* across
/// `[[transport]]` entries) is supported on hosted / gateway-Linux targets,
/// but typically cannot link on an embedded target — warn rather than fail so
/// the user can confirm the target really does provide every backend.
fn collect_plan_warnings(plan: &NrosPlan) -> Vec<String> {
    let mut warnings = Vec::new();
    let rmws = linked_rmw_set(&plan.build);
    if rmws.len() > 1 && !plan_target_is_hosted(&plan.build) {
        warnings.push(format!(
            "target `{}` links {} RMW backends ({}) into one binary; cross-RMW \
             in-binary bridging is supported on hosted/gateway targets but may not \
             link on this embedded target",
            plan.build.target,
            rmws.len(),
            rmws.iter().copied().collect::<Vec<_>>().join(", "),
        ));
    }
    warnings
}

/// The distinct RMW backends linked into the binary: each `[[transport]]`'s
/// `rmw` (falling back to `build.rmw`), or just `build.rmw` for a zero-config
/// single-transport build.
fn linked_rmw_set(build: &PlanBuildOptions) -> std::collections::BTreeSet<&str> {
    let mut set = std::collections::BTreeSet::new();
    if build.transports.is_empty() {
        set.insert(build.rmw.as_str());
    } else {
        for transport in &build.transports {
            set.insert(transport.rmw.as_deref().unwrap_or(build.rmw.as_str()));
        }
    }
    set
}

/// Whether the build target is a hosted (OS-backed) target — where linking
/// multiple RMW backends into one process is routine.
fn plan_target_is_hosted(build: &PlanBuildOptions) -> bool {
    matches!(build.board.as_str(), "native" | "posix")
        || build.target.contains("linux")
        || build.target.contains("darwin")
        || build.target.contains("apple")
        || build.target.contains("windows")
}

fn validate_plan(plan: &NrosPlan) -> Vec<String> {
    let mut errors = Vec::new();
    let mut component_ids = HashSet::new();
    let mut instance_ids = HashSet::new();
    let mut sched_context_ids = HashSet::new();
    let mut interface_ids = HashSet::new();
    let mut component_lookup = HashSet::new();
    let mut sched_context_lookup = HashSet::new();
    let mut entity_lookup = HashSet::new();
    let mut interface_lookup = HashMap::new();

    for component in &plan.components {
        push_duplicate(
            &mut errors,
            "duplicate-component-id",
            &component.id,
            &mut component_ids,
        );
        component_lookup.insert(component.id.as_str());
    }
    for context in &plan.sched_contexts {
        push_duplicate(
            &mut errors,
            "duplicate-sched-context-id",
            &context.id,
            &mut sched_context_ids,
        );
        sched_context_lookup.insert(context.id.as_str());
    }
    for interface in &plan.interfaces {
        push_duplicate(
            &mut errors,
            "duplicate-interface-id",
            &interface.id,
            &mut interface_ids,
        );
        interface_lookup.insert(interface.id.as_str(), &interface.interface);
    }

    for instance in &plan.instances {
        push_duplicate(
            &mut errors,
            "duplicate-instance-id",
            &instance.id,
            &mut instance_ids,
        );
        if !component_lookup.contains(instance.component.as_str()) {
            errors.push(format!(
                "missing-component-reference: instance {} references {}",
                instance.id, instance.component
            ));
        }

        let mut node_ids = HashSet::new();
        let mut local_entity_ids = HashSet::new();
        let mut callback_ids = HashSet::new();
        for node in &instance.nodes {
            push_duplicate(&mut errors, "duplicate-node-id", &node.id, &mut node_ids);
            for entity in &node.entities {
                let entity_id = plan_entity_id(entity);
                push_duplicate(
                    &mut errors,
                    "duplicate-entity-id",
                    entity_id,
                    &mut local_entity_ids,
                );
                entity_lookup.insert(entity_id);
            }
        }
        for callback in &instance.callbacks {
            push_duplicate(
                &mut errors,
                "duplicate-callback-id",
                &callback.id,
                &mut callback_ids,
            );
            if !sched_context_lookup.contains(callback.sched_context.as_str()) {
                errors.push(format!(
                    "missing-sched-context: callback {} references {}",
                    callback.id, callback.sched_context
                ));
            }
        }
        for binding in &instance.sched_bindings {
            if !callback_ids.contains(binding.callback.as_str()) {
                errors.push(format!(
                    "missing-sched-callback: binding references {}",
                    binding.callback
                ));
            }
            if !sched_context_lookup.contains(binding.context.as_str()) {
                errors.push(format!(
                    "missing-sched-context: binding for {} references {}",
                    binding.callback, binding.context
                ));
            }
        }
        for parameter in &instance.parameters {
            if !node_ids.contains(parameter.node.as_str()) {
                errors.push(format!(
                    "missing-parameter-node: parameter {} references {}",
                    parameter.name, parameter.node
                ));
            }
        }
    }

    for interface in &plan.interfaces {
        for entity_id in &interface.used_by {
            if !entity_lookup.contains(entity_id.as_str()) {
                errors.push(format!(
                    "missing-interface-entity: interface {} references {}",
                    interface.id, entity_id
                ));
            }
        }
    }
    for instance in &plan.instances {
        for node in &instance.nodes {
            for entity in &node.entities {
                let Some(entity_interface) = plan_entity_interface(entity) else {
                    continue;
                };
                let entity_id = plan_entity_id(entity);
                let interface_id = interface_id(entity_interface);
                match interface_lookup.get(interface_id.as_str()) {
                    Some(table_interface) if *table_interface == entity_interface => {}
                    Some(_) => errors.push(format!(
                        "interface-ref-mismatch: entity {} uses {}",
                        entity_id, interface_id
                    )),
                    None => errors.push(format!(
                        "missing-interface-ref: entity {} uses {}",
                        entity_id, interface_id
                    )),
                }
                if !plan.interfaces.iter().any(|interface| {
                    interface.id == interface_id
                        && interface.used_by.iter().any(|id| id == entity_id)
                }) {
                    errors.push(format!(
                        "missing-interface-usage: entity {} not listed under {}",
                        entity_id, interface_id
                    ));
                }
            }
        }
    }

    errors
}

fn push_duplicate<'a>(
    errors: &mut Vec<String>,
    code: &str,
    id: &'a str,
    seen: &mut HashSet<&'a str>,
) {
    if !seen.insert(id) {
        errors.push(format!("{code}: {id}"));
    }
}

fn plan_entity_id(entity: &PlanEntity) -> &str {
    match entity {
        PlanEntity::Publisher { id, .. }
        | PlanEntity::Subscriber { id, .. }
        | PlanEntity::Timer { id, .. }
        | PlanEntity::ServiceServer { id, .. }
        | PlanEntity::ServiceClient { id, .. }
        | PlanEntity::ActionServer { id, .. }
        | PlanEntity::ActionClient { id, .. } => id,
    }
}

fn plan_entity_interface(entity: &PlanEntity) -> Option<&InterfaceRef> {
    match entity {
        PlanEntity::Publisher { interface, .. }
        | PlanEntity::Subscriber { interface, .. }
        | PlanEntity::ServiceServer { interface, .. }
        | PlanEntity::ServiceClient { interface, .. }
        | PlanEntity::ActionServer { interface, .. }
        | PlanEntity::ActionClient { interface, .. } => Some(interface),
        PlanEntity::Timer { .. } => None,
    }
}

fn interface_id(interface: &InterfaceRef) -> String {
    format!("{}/{}", interface.package, interface.name)
}

fn parse_launch_args(args: &[String]) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for arg in args {
        let Some((key, value)) = arg.split_once(":=").or_else(|| arg.split_once('=')) else {
            return Err(eyre!(
                "invalid launch argument `{arg}`: expected name:=value or name=value"
            ));
        };
        out.insert(key.to_string(), value.to_string());
    }
    Ok(out)
}

fn load_or_parse_record(
    launch_file: &Path,
    workspace_root: &Path,
    record_file: Option<&Path>,
    launch_args: HashMap<String, String>,
) -> Result<Value> {
    if let Some(record_file) = record_file {
        let raw = fs::read_to_string(record_file)
            .wrap_err_with(|| format!("failed to read record {}", record_file.display()))?;
        return serde_json::from_str(&raw)
            .wrap_err_with(|| format!("invalid record JSON {}", record_file.display()));
    }
    parse_launch_file_record(launch_file, workspace_root, launch_args)
}

/// Phase 212.M-F.20 — synthesize a throwaway ament prefix from the workspace
/// pkg-index so the launch parser's `$(find-pkg-share <pkg>)` substitution
/// resolves in-tree workspace packages that were never `colcon install`ed.
///
/// `play_launch_parser` resolves `find-pkg-share` by walking `AMENT_PREFIX_PATH`
/// for a `<prefix>/share/<pkg>` dir backed by an ament-index resource marker.
/// An in-tree development workspace has no such install tree, so a launch file
/// that includes `<include file="$(find-pkg-share other_pkg)/launch/x.xml"/>`
/// fails with `Package not found` (which is why O.5 historically sidestepped
/// with a relative include). Here we build the `package.xml` pkg-index
/// (`build_pkg_index`, the same surface M-F.17 / N.10 drive) and lay down a
/// minimal valid prefix in a `TempDir`:
///
/// ```text
/// <tmp>/share/ament_index/resource_index/packages/<pkg>   (empty marker)
/// <tmp>/share/<pkg>  ->  <workspace>/.../<pkg>             (symlink to source)
/// ```
///
/// The caller prepends `<tmp>` to `AMENT_PREFIX_PATH` for the parser process so
/// workspace packages win over any installed ones. Returns the live `TempDir`
/// (its prefix path) — the directory is removed when it drops, after the parser
/// has run. Best-effort: a missing/empty index returns `Ok(None)` (the parser
/// falls back to the ambient `AMENT_PREFIX_PATH` exactly as before).
fn synthesize_workspace_ament_prefix(workspace_root: &Path) -> Result<Option<tempfile::TempDir>> {
    let index = match crate::pkg_index::build_pkg_index(workspace_root) {
        Ok(index) => index,
        // No discoverable workspace (no package.xml walk root) — not this
        // helper's concern; leave AMENT_PREFIX_PATH untouched.
        Err(_) => return Ok(None),
    };
    let pkgs: Vec<(String, PathBuf)> = index
        .pkgs()
        .map(|(name, dir)| (name.to_string(), dir.to_path_buf()))
        .collect();
    if pkgs.is_empty() {
        return Ok(None);
    }

    let prefix = tempfile::Builder::new()
        .prefix("nros-ament-prefix-")
        .tempdir()
        .wrap_err("failed to create temp ament prefix")?;
    let share = prefix.path().join("share");
    let marker_dir = share.join("ament_index/resource_index/packages");
    fs::create_dir_all(&marker_dir).wrap_err("failed to create ament resource-index dir")?;

    for (name, dir) in &pkgs {
        // Absolute symlink target so resolution is independent of the parser's
        // cwd. `dir` is already absolute when `workspace_root` is, but canonical
        // is cheap insurance.
        let target = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        let link = share.join(name);
        // `share/<pkg>` -> pkg source dir (carries `launch/`, `config/`, …).
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)
            .wrap_err_with(|| format!("failed to symlink share/{name} -> {}", target.display()))?;
        #[cfg(not(unix))]
        {
            // Non-unix hosts: copy the launch dir if present (best-effort).
            let _ = (&target, &link);
        }
        // Empty ament-index marker so `<pkg>` is a known ament resource.
        fs::write(marker_dir.join(name), b"")
            .wrap_err_with(|| format!("failed to write ament marker for {name}"))?;
    }

    Ok(Some(prefix))
}

/// Resolve a launch file to a record by shelling out to the external
/// `play_launch_parser` binary (Phase 195.A). nano-ros keeps the `nros` binary
/// itself free of the pyo3/`libpython` embedding (the launch parser embeds
/// CPython to execute `.launch.py`); it lives in the separate, python-bearing
/// `play_launch_parser` tool (`pip install play-launch-parser` or its binary).
/// The build system runs this internally to produce the record; `--record` is
/// not a user-facing surface. Override the binary via `NROS_PLAY_LAUNCH_PARSER`.
fn parse_launch_file_record(
    launch_file: &Path,
    workspace_root: &Path,
    launch_args: HashMap<String, String>,
) -> Result<Value> {
    let bin = std::env::var("NROS_PLAY_LAUNCH_PARSER")
        .unwrap_or_else(|_| "play_launch_parser".to_string());
    let mut cmd = Command::new(&bin);

    // Phase 212.M-F.20 — let `$(find-pkg-share <pkg>)` resolve in-tree
    // workspace packages that were never installed. Synthesize a throwaway
    // ament prefix from the `package.xml` pkg-index and prepend it to the
    // parser's `AMENT_PREFIX_PATH` (workspace shadows any installed copy).
    // `_ament_prefix` keeps the `TempDir` alive until after `cmd.output()`.
    let _ament_prefix = synthesize_workspace_ament_prefix(workspace_root)?;
    if let Some(prefix) = &_ament_prefix {
        let mut ament = prefix.path().as_os_str().to_os_string();
        if let Some(existing) = std::env::var_os("AMENT_PREFIX_PATH") {
            if !existing.is_empty() {
                ament.push(":");
                ament.push(existing);
            }
        }
        cmd.env("AMENT_PREFIX_PATH", ament);
    }
    // `<include>` recursion-safety knobs (Phase 211.J):
    //
    // * `--strict-includes` — orchestration cannot tolerate a silently-dropped
    //   include branch (the dropped sub-tree's nodes would simply vanish from
    //   the plan), so the planner always runs the parser in strict mode. This
    //   flips the parser default of warn-and-skip into a hard
    //   `ParseError::CircularInclude` that surfaces as a non-zero exit + the
    //   include chain in stderr — what every `nros plan` caller actually wants.
    //
    // * `--max-include-depth` — opt-in cap. The parser default is 100
    //   (generous enough to never false-positive on Autoware); set
    //   `NROS_PLAY_LAUNCH_MAX_INCLUDE_DEPTH=<N>` to tighten or loosen.
    //   16 is the 211.J-proposed default for orchestration but we keep the
    //   parser's 100 unless the env var is explicitly set, so we don't break
    //   any existing user's plan.
    cmd.arg("--strict-includes");
    if let Ok(depth) = std::env::var("NROS_PLAY_LAUNCH_MAX_INCLUDE_DEPTH") {
        cmd.arg("--max-include-depth").arg(depth);
    }
    cmd.arg("file").arg(launch_file);
    for (k, v) in &launch_args {
        cmd.arg(format!("{k}:={v}"));
    }
    let output = cmd.output().map_err(|err| {
        eyre!(
            "failed to run `{bin}` (launch parser) for {}: {err}. Install it \
             (`pip install play-launch-parser`, or build the play_launch_parser \
             binary) and put it on PATH, or set NROS_PLAY_LAUNCH_PARSER=<path>.",
            launch_file.display()
        )
    })?;
    if !output.status.success() {
        bail!(
            "{bin} failed for {} (exit {}):\n{}",
            launch_file.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).wrap_err_with(|| {
        format!(
            "invalid record JSON from {bin} for {}",
            launch_file.display()
        )
    })
}

fn record_array<'a>(record: &'a Value, key: &str) -> &'a [Value] {
    record
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn metadata_paths(
    options: &PlanOptions,
    workspace: &Workspace,
    metadata_dir: &Path,
) -> Vec<PathBuf> {
    let mut paths = options.metadata_files.clone();
    paths.extend(workspace.source_metadata_files());
    if metadata_dir.is_dir()
        && let Ok(entries) = fs::read_dir(metadata_dir)
    {
        paths.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json")),
        );
    }
    unique_paths(paths)
}

fn load_json_artifacts(paths: &[PathBuf], label: &str) -> Result<Vec<JsonArtifact>> {
    paths
        .iter()
        .map(|path| {
            let raw = fs::read_to_string(path)
                .wrap_err_with(|| format!("failed to read {label} {}", path.display()))?;
            let value = serde_json::from_str(&raw)
                .wrap_err_with(|| format!("invalid {label} JSON {}", path.display()))?;
            Ok(JsonArtifact {
                path: path.clone(),
                value,
            })
        })
        .collect()
}

fn preserve_metadata(metadata: &[JsonArtifact], metadata_dir: &Path) -> Result<()> {
    for artifact in metadata {
        // Phase 212.M-F.17 — synthetic artifacts derived from cargo metadata
        // carry a `Cargo.toml` source path; preserving them as `Cargo.toml`
        // files inside the JSON metadata dir would (a) confuse downstream
        // readers that expect `*.json`, and (b) collide across packages.
        // Skip them: the planner consumes the live `metadata` slice, the
        // preserved-to-disk view is for sidecar JSON only.
        if artifact
            .value
            .get("synthetic")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(file_name) = artifact.path.file_name() else {
            continue;
        };
        let dest = metadata_dir.join(file_name);
        if dest != artifact.path {
            fs::write(dest, serde_json::to_string_pretty(&artifact.value)?)?;
        }
    }
    Ok(())
}

fn schema_plan_json(
    options: &PlanOptions,
    record_path: &Path,
    instances: &[Value],
    executables: &[Value],
    metadata: &[JsonArtifact],
    overlays: &[Value],
    system_toml: Option<&Path>,
    build: Value,
) -> Value {
    let components = schema_components(metadata);
    // Phase 172.G — scheduling tiers come from nros.toml `[[scheduling.contexts]]`
    // (author-declared, not inferred); callbacks bind to them by `group` name.
    let (declared_contexts, declared_by_id) = collect_sched_contexts(overlays);
    let plan_instances = instances
        .iter()
        .map(|instance| schema_instance(instance, &declared_by_id))
        .collect::<Vec<_>>();
    let interfaces = schema_interfaces(&plan_instances);
    let callback_chains = infer_callback_chains(&plan_instances);
    let callback_groups = infer_callback_groups(&plan_instances, &callback_chains);

    // Emit the declared tiers; append the implicit `default_executor` when it is
    // the catch-all for any unbound callback (or when no tiers were declared, so
    // single-tier plans stay byte-identical to pre-172.G).
    let uses_default = plan_instances.iter().any(|inst| {
        inst.get("callbacks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|cb| cb.get("sched_context").and_then(Value::as_str) == Some("default_executor"))
    });
    let mut sched_contexts = declared_contexts;
    if (sched_contexts.is_empty() || uses_default)
        && !declared_by_id.contains_key("default_executor")
    {
        sched_contexts.push(default_sched_context());
    }

    let mut plan = json!({
        "version": 2,
        "system": options.system_pkg,
        "trace": {
            "system_config": options.nros_toml_files.first().map(|p| p.display().to_string()).unwrap_or_else(|| "nros.toml".to_string()),
            "launch_record": record_path.display().to_string(),
            "generated_by": "nros plan",
        },
        "components": components,
        "instances": plan_instances,
        "interfaces": interfaces,
        "sched_contexts": sched_contexts,
        "callback_chains": callback_chains,
        "callback_groups": callback_groups,
    });
    // Phase 172.A — append the optional lifecycle block (before `build`, to
    // match the NrosPlan field order) only when nros.toml declares [lifecycle];
    // a non-lifecycle plan stays byte-identical to pre-172.A.
    let obj = plan.as_object_mut().expect("plan is an object");
    if let Some(lifecycle) = collect_lifecycle(overlays) {
        obj.insert("lifecycle".to_string(), lifecycle);
    }
    // Phase 172.I — optional shared-state regions, before `build` (NrosPlan
    // field order); absent ⇒ omitted, plan stays byte-identical.
    let shared_state = collect_shared_state(overlays);
    if !shared_state.is_empty() {
        obj.insert("shared_state".to_string(), json!(shared_state));
    }
    // Phase 172.H — optional parameter-override persistence, before `build`
    // (NrosPlan field order); absent ⇒ omitted, plan stays byte-identical.
    if let Some(pp) = collect_param_persistence(overlays) {
        obj.insert("param_persistence".to_string(), pp);
    }
    // Phase 254 — capability axes ([param_services], [safety]) prefer the typed
    // `system.toml` (the SSoT both codegen paths read); the per-package `nros.toml`
    // overlay block is a DEPRECATED fallback (warns), kept one release for migration.
    let system_caps = system_toml
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str::<super::cargo_metadata_schema::SystemToml>(&s).ok());

    // Phase 250 (Wave 3) — parameter-server capability, before `build` (NrosPlan
    // field order); absent ⇒ omitted, plan stays byte-identical.
    let param_services = system_caps
        .as_ref()
        .and_then(|s| s.param_services.as_ref())
        .filter(|p| p.enabled)
        .map(|_| json!({}))
        .or_else(|| {
            collect_param_services(overlays).inspect(|_| {
                eprintln!(
                    "warning: [param_services] in nros.toml is deprecated (phase-254); \
                     declare it in the bringup system.toml"
                );
            })
        });
    if let Some(ps) = param_services {
        obj.insert("param_services".to_string(), ps);
    }
    // Phase 250 (Wave 1) — E2E-safety capability, before `build` (NrosPlan field
    // order); absent ⇒ omitted, plan stays byte-identical.
    let safety = system_caps
        .as_ref()
        .and_then(|s| s.safety.as_ref())
        .filter(|s| s.enabled)
        .map(|s| json!({ "crc": s.crc }))
        .or_else(|| {
            collect_safety(overlays).inspect(|_| {
                eprintln!(
                    "warning: [safety] in nros.toml is deprecated (phase-254); \
                     declare it in the bringup system.toml"
                );
            })
        });
    if let Some(safety) = safety {
        obj.insert("safety".to_string(), safety);
    }
    // Phase 211.E — `<executable>` spawn entries. Skip-when-empty so plans
    // without any `<executable>` stay byte-identical to pre-211.E.
    if !executables.is_empty() {
        let plan_executables = executables
            .iter()
            .map(schema_executable)
            .collect::<Vec<_>>();
        obj.insert("executables".to_string(), json!(plan_executables));
    }
    obj.insert("build".to_string(), build);
    plan
}

/// Phase 173.5 — assemble the plan `build` block from the nros.toml
/// overlays. Pre-173.5 defaults (native / zenoh / debug) hold when a
/// key is absent, so a plan with no `[build]` / `[[transport]]` is
/// byte-identical to before. Later overlays override earlier ones.
///
/// TOML `[build]` → the board / target / rmw / profile / features / cfg
/// fields; TOML `[[transport]]` (array key `transport`) → the
/// `transports` field. Unknown keys are caught downstream by
/// `PlanBuildOptions`'s `deny_unknown_fields`.
fn schema_build_json(overlays: &[Value], system_toml: Option<&Path>) -> Value {
    let mut build = json!({
        "target": "x86_64-unknown-linux-gnu",
        "board": "native",
        "rmw": "zenoh",
        "profile": "debug",
        "features": [],
        "cfg": {},
        "transports": [],
    });
    let obj = build.as_object_mut().expect("build is an object");
    let mut overlay_had_rmw = false;
    for overlay in overlays {
        if let Some(Value::Object(b)) = overlay.get("build") {
            if b.contains_key("rmw") {
                overlay_had_rmw = true;
            }
            for key in [
                "target", "board", "rmw", "profile", "features", "cfg", "optimize", "cargo", "cc",
            ] {
                if let Some(v) = b.get(key) {
                    obj.insert(key.to_string(), v.clone());
                }
            }
        }
        // `[[transport]]` in nros.toml deserialises to the array-valued
        // key `transport`; the plan field is `transports`.
        if let Some(transports) = overlay.get("transport") {
            obj.insert("transports".to_string(), transports.clone());
        }
    }
    // Phase 255 — `[system].rmw` (resolved) is the SSoT and wins over the
    // DEPRECATED `[build].rmw` overlay. `[deploy.<t>].rmw` / `--rmw` plumb in once
    // the planner is target-aware (issue 0076 §A); today both paths resolve to
    // `[system].rmw` with no target, unifying them.
    if let Some(sys) = system_toml
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str::<super::cargo_metadata_schema::SystemToml>(&s).ok())
    {
        if overlay_had_rmw {
            eprintln!(
                "warning: [build].rmw in nros.toml is deprecated (phase-255); declare rmw in \
                 the bringup system.toml ([system].rmw / [deploy.<t>].rmw)"
            );
        }
        obj.insert("rmw".to_string(), json!(sys.resolved_rmw(None, None)));
    }
    build
}

fn schema_components(metadata: &[JsonArtifact]) -> Vec<Value> {
    // Phase 172.U — dedup by component id: the same component's source metadata
    // can reach the planner from more than one place (e.g. a collected copy in
    // the build metadata dir + the in-package `metadata/` file a
    // `component_nros.toml` declares), and they describe one component. Keep
    // the first; identical duplicates would otherwise trip
    // `duplicate-component-id`.
    let mut seen = HashSet::new();
    metadata
        .iter()
        .filter_map(|artifact| {
            let package = string_field(&artifact.value, &["package"]).unwrap_or("unknown");
            let component =
                string_field(&artifact.value, &["component", "executable"]).unwrap_or("unknown");
            let id = format!("{package}::{component}");
            if !seen.insert(id.clone()) {
                return None;
            }
            let language = string_field(&artifact.value, &["language"]).unwrap_or("rust");
            Some(json!({
                "id": id,
                "package": package,
                "component": component,
                "language": language,
                "source_metadata": artifact.path.display().to_string(),
                "component_config": null,
            }))
        })
        .collect()
}

fn schema_instance(instance: &Value, declared: &BTreeMap<String, Value>) -> Value {
    let id = instance
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("instance");
    let package = instance
        .get("package")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let executable = instance
        .get("executable")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let namespace = instance
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("/");
    let launch_name = instance
        .get("node_name")
        .and_then(Value::as_str)
        .unwrap_or(executable);
    let source_nodes = instance
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            vec![json!({
                "id": "node",
                "resolved_name": launch_name,
                "namespace": namespace,
            })]
        });
    let raw_entities = instance
        .get("entities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let callbacks = schema_callbacks(id, instance.get("callbacks"), declared);
    let callback_lookup = CallbackIdLookup::from_callbacks(&callbacks);
    let nodes = schema_nodes(id, &source_nodes, &raw_entities, &callback_lookup);
    let sched_bindings = schema_sched_bindings(&callbacks, declared);
    let default_node_id = source_nodes
        .first()
        .map(|node| schema_node_id(id, node))
        .unwrap_or_else(|| format!("{id}/node"));
    // Phase 211.B — map the intermediate `launch_kind` onto the public
    // schema's `kind`: "node" / "container" / "composable_node".
    let kind = match instance.get("launch_kind").and_then(Value::as_str) {
        Some("container") => "container",
        Some("load_node") => "composable_node",
        _ => "node",
    };
    let container_id = instance
        .get("container_id")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let mut out = json!({
        "id": id,
        "component": format!("{package}::{executable}"),
        "package": package,
        "executable": executable,
        // Phase 211.B — defaults to "node" (matches PlanInstance::default_kind);
        // emitted explicitly so the wire shape always carries the kind.
        "kind": kind,
        "launch_name": launch_name,
        "namespace": namespace,
        "remaps": schema_remaps(instance.get("remaps")),
        // Phase 211.E — `<set_env>` / `<env>` declarations from the launch
        // surface here as `[{name, value}, …]`. Always emitted (empty when
        // nothing is declared) so deploy iterates uniformly.
        "env": schema_env(instance.get("env")),
        "nodes": nodes,
        "callbacks": callbacks,
        "parameters": schema_parameters(&default_node_id, instance.get("parameters")),
        "sched_bindings": sched_bindings,
        "trace": {
            "launch_record_entity": format!("record://{id}"),
            "source_metadata": instance.get("source_metadata").and_then(Value::as_str).unwrap_or(""),
        },
    });
    // Phase 211.B — `container_id` is `skip_serializing_if = "Option::is_none"`
    // on the schema struct, so we only emit it when actually set (composable
    // children); plain nodes + containers themselves stay byte-compat.
    if let Some(parent_id) = container_id {
        out.as_object_mut()
            .expect("schema_instance produces object")
            .insert("container_id".to_string(), json!(parent_id));
    }
    // Phase 211.H — `qos_overrides` is `skip_serializing_if = "Vec::is_empty"`
    // on the schema struct, so only emit it when the launch carried
    // `qos_overrides.*` params; plans without them stay byte-compatible.
    let qos_overrides = schema_qos_overrides(instance.get("parameters"));
    if !qos_overrides.is_empty() {
        out.as_object_mut()
            .expect("schema_instance produces object")
            .insert("qos_overrides".to_string(), json!(qos_overrides));
    }
    // Phase 211.F — lower the launch `<node machine="…">` (recorded by
    // play_launch_parser as `machine`) into `host_id`. `skip_serializing_if`
    // on the schema, so only emitted for multi-host launches; single-host
    // plans stay byte-compatible.
    if let Some(machine) = instance
        .get("machine")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
    {
        out.as_object_mut()
            .expect("schema_instance produces object")
            .insert("host_id".to_string(), json!(machine));
    }
    out
}

fn schema_nodes(
    instance_id: &str,
    source_nodes: &[Value],
    entities: &[Value],
    callback_lookup: &CallbackIdLookup,
) -> Vec<Value> {
    source_nodes
        .iter()
        .map(|node| {
            let source_node = node.get("id").and_then(Value::as_str).unwrap_or("node");
            let node_entities = entities
                .iter()
                .filter(|entity| {
                    entity
                        .get("source_node")
                        .and_then(Value::as_str)
                        .unwrap_or("node")
                        == source_node
                })
                .filter_map(|entity| schema_entity(instance_id, entity, callback_lookup))
                .collect::<Vec<_>>();
            let mut out = json!({
                "id": schema_node_id(instance_id, node),
                "source_node": source_node,
                "resolved_name": node.get("resolved_name").and_then(Value::as_str).unwrap_or(""),
                "namespace": node.get("namespace").and_then(Value::as_str).unwrap_or("/"),
                "entities": node_entities,
            });
            if let Some(domain_id) = node.get("domain_id").and_then(Value::as_u64) {
                out.as_object_mut()
                    .expect("schema node is an object")
                    .insert("domain_id".to_string(), json!(domain_id));
            }
            copy_json_field(&mut out, node, "source_default_name");
            copy_json_field(&mut out, node, "declaration_slot");
            copy_json_field(&mut out, node, "source");
            out
        })
        .collect()
}

/// Phase 172.G — the implicit single tier. Emitted when nros.toml declares no
/// `[[scheduling.contexts]]`, or as the catch-all for callbacks whose `group`
/// matches no declared tier. Byte-identical to the pre-172.G hardcoded context
/// so single-tier systems keep their exact plan output.
fn default_sched_context() -> Value {
    json!({
        "id": "default_executor",
        "executor": "single_threaded",
        "class": "best_effort",
        "priority": null,
        "period_ms": null,
        "budget_ms": null,
        "deadline_ms": null,
        "deadline_policy": "ignore",
        "stack_size": null,
        "core": null,
        "task": null,
    })
}

/// Phase 172.G — normalise one nros.toml `[[scheduling.contexts]]` entry into a
/// plan `sched_context` value, filling every optional key so the result
/// round-trips through `PlanSchedContext` (which requires all keys present).
/// The TOML field names + value encodings already match the plan schema
/// (`config::SchedContextConfig` mirrors `PlanSchedContext`), so this only
/// supplies defaults for absent keys.
fn normalize_sched_context(ctx: &Value) -> Value {
    let str_or = |key: &str, default: &str| {
        ctx.get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string()
    };
    let val_or_null = |key: &str| ctx.get(key).cloned().unwrap_or(Value::Null);
    json!({
        "id": str_or("id", ""),
        "executor": str_or("executor", "single_threaded"),
        "class": str_or("class", "best_effort"),
        "priority": val_or_null("priority"),
        "period_ms": val_or_null("period_ms"),
        "budget_ms": val_or_null("budget_ms"),
        "deadline_ms": val_or_null("deadline_ms"),
        "deadline_policy": str_or("deadline_policy", "ignore"),
        "stack_size": val_or_null("stack_size"),
        "core": val_or_null("core"),
        "task": val_or_null("task"),
    })
}

/// Phase 172.G — collect the declared scheduling tiers from the nros.toml
/// overlays. Each `[[scheduling.contexts]]` becomes a plan `sched_context`,
/// keyed by id (declaration order preserved; a later overlay redeclaring an id
/// overrides the earlier one — last-wins, mirroring `schema_build_json`).
/// Returns the ordered context values plus an id→context map for binding
/// lookups.
fn collect_sched_contexts(overlays: &[Value]) -> (Vec<Value>, BTreeMap<String, Value>) {
    let mut order: Vec<String> = Vec::new();
    let mut by_id: BTreeMap<String, Value> = BTreeMap::new();
    for overlay in overlays {
        let Some(contexts) = overlay
            .get("scheduling")
            .and_then(|s| s.get("contexts"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for ctx in contexts {
            let Some(id) = ctx.get("id").and_then(Value::as_str) else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            if !by_id.contains_key(id) {
                order.push(id.to_string());
            }
            by_id.insert(id.to_string(), normalize_sched_context(ctx));
        }
    }
    let contexts = order.iter().map(|id| by_id[id].clone()).collect();
    (contexts, by_id)
}

/// Phase 172.A — read the nros.toml `[lifecycle]` block (last overlay wins).
/// Returns the plan lifecycle value `{ "autostart": <policy> }` when the block
/// is present; `None` keeps the binary's node a plain (unmanaged) node. The
/// `autostart` policy defaults to `none` (register services, stay
/// `Unconfigured`) when the key is omitted; an unknown value passes through and
/// is rejected by `nros check` (NrosPlan parse).
fn collect_lifecycle(overlays: &[Value]) -> Option<Value> {
    let mut out = None;
    for overlay in overlays {
        if let Some(lc) = overlay.get("lifecycle") {
            let autostart = lc
                .get("autostart")
                .and_then(Value::as_str)
                .unwrap_or("none");
            out = Some(json!({ "autostart": autostart }));
        }
    }
    out
}

/// Phase 172.H — read the nros.toml `[param_persistence]` block (last overlay
/// wins). Returns `{ "backend": <kind>, "path": <loc> }` when a non-empty
/// `path` is present (`backend` defaults to `"file"`); `None` keeps the binary
/// free of parameter services. An unknown backend passes through and is the
/// generator's concern.
fn collect_param_persistence(overlays: &[Value]) -> Option<Value> {
    let mut out = None;
    for overlay in overlays {
        if let Some(table) = overlay.get("param_persistence").and_then(Value::as_object) {
            let backend = table
                .get("backend")
                .and_then(Value::as_str)
                .unwrap_or("file");
            let path = table.get("path").and_then(Value::as_str).unwrap_or("");
            if !path.is_empty() {
                out = Some(json!({ "backend": backend, "path": path }));
            }
        }
    }
    out
}

/// Phase 250 (Wave 3) — read the nros.toml `[param_services]` block (last
/// overlay wins). Returns `{}` when present and not disabled (`enabled = false`);
/// `None` keeps the binary free of the parameter SERVER. The presence is the
/// enable signal the generator lowers to `nros/param-services` (the runtime then
/// registers the 6 ROS 2 param services on the first declared parameter).
fn collect_param_services(overlays: &[Value]) -> Option<Value> {
    let mut out = None;
    for overlay in overlays {
        if let Some(table) = overlay.get("param_services").and_then(Value::as_object) {
            let enabled = table
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            out = if enabled { Some(json!({})) } else { None };
        }
    }
    out
}

/// Phase 250 (Wave 1) — read the nros.toml `[safety]` block (last overlay
/// wins). Returns `{ "crc": <bool> }` when the block is present and not
/// explicitly disabled (`enabled = false`); `None` keeps the binary free of the
/// E2E-safety capability. `crc` defaults to true. The presence of the result is
/// the enable signal the generator lowers to `nros/safety-e2e`.
fn collect_safety(overlays: &[Value]) -> Option<Value> {
    let mut out = None;
    for overlay in overlays {
        if let Some(table) = overlay.get("safety").and_then(Value::as_object) {
            let enabled = table
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if enabled {
                let crc = table.get("crc").and_then(Value::as_bool).unwrap_or(true);
                out = Some(json!({ "crc": crc }));
            } else {
                out = None;
            }
        }
    }
    out
}

/// Phase 172.I — collect `nros.toml` `[[shared_state]]` entries (array key
/// `shared_state`) into the plan's `shared_state`. Entries with an empty id or
/// zero bytes are dropped. Empty ⇒ no shared state (byte-identical plan).
fn collect_shared_state(overlays: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for overlay in overlays {
        if let Some(Value::Array(regions)) = overlay.get("shared_state") {
            for region in regions {
                let id = region.get("id").and_then(Value::as_str).unwrap_or("");
                let bytes = region.get("bytes").and_then(Value::as_u64).unwrap_or(0);
                if !id.is_empty() && bytes > 0 {
                    out.push(json!({ "id": id, "bytes": bytes }));
                }
            }
        }
    }
    out
}

fn schema_callbacks(
    instance_id: &str,
    value: Option<&Value>,
    declared: &BTreeMap<String, Value>,
) -> Vec<Value> {
    let Some(Value::Array(callbacks)) = value else {
        return Vec::new();
    };
    callbacks
        .iter()
        .filter_map(|callback| {
            let source_callback = callback.get("id").and_then(Value::as_str)?;
            if source_callback.is_empty() {
                return None;
            }
            let source = callback.get("source").cloned().unwrap_or_else(|| {
                json!({
                    "artifact": "source-metadata.json",
                    "line": null,
                    "column": null,
                })
            });
            // Phase 172.G — a callback's `group` names its scheduling tier
            // ("group name = tier id"). Bind to the declared context of that
            // name when one exists; otherwise fall back to `default_executor`.
            let group = callback
                .get("group")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let sched_context = if declared.contains_key(group) {
                group
            } else {
                "default_executor"
            };
            let mut out = json!({
                "id": schema_callback_id(instance_id, callback, source_callback),
                "source_callback": source_callback,
                "group": group,
                "sched_context": sched_context,
                "source": source,
            });
            copy_json_field(&mut out, callback, "declaration_slot");
            Some(out)
        })
        .collect()
}

#[derive(Debug, Default)]
struct CallbackIdLookup {
    by_source: HashMap<String, String>,
    by_slot: HashMap<u64, String>,
}

impl CallbackIdLookup {
    fn from_callbacks(callbacks: &[Value]) -> Self {
        let mut lookup = Self::default();
        for callback in callbacks {
            let Some(id) = callback.get("id").and_then(Value::as_str) else {
                continue;
            };
            if let Some(slot) = declaration_slot(callback) {
                if let Some(source_callback) =
                    callback.get("source_callback").and_then(Value::as_str)
                {
                    lookup
                        .by_source
                        .insert(source_callback.to_string(), id.to_string());
                }
                lookup.by_slot.insert(slot, id.to_string());
            }
        }
        lookup
    }

    fn resolve(&self, source_callback: &str, callback_slot: Option<u64>) -> String {
        callback_slot
            .and_then(|slot| self.by_slot.get(&slot))
            .or_else(|| self.by_source.get(source_callback))
            .cloned()
            .unwrap_or_else(|| source_callback.to_string())
    }
}

/// Phase 172.G — one `sched_binding` per callback, binding it to the tier its
/// `group` resolved to in [`schema_callbacks`]. A binding onto a declared
/// nros.toml tier carries that tier's priority + `source: "nros.toml"`; the
/// `default_executor` fall-back keeps the pre-172.G `priority: null` +
/// `source: "source_metadata"` so single-tier plans stay byte-identical.
fn schema_sched_bindings(callbacks: &[Value], declared: &BTreeMap<String, Value>) -> Vec<Value> {
    callbacks
        .iter()
        .filter_map(|callback| {
            let id = callback.get("id").and_then(Value::as_str)?;
            let context = callback
                .get("sched_context")
                .and_then(Value::as_str)
                .unwrap_or("default_executor");
            match declared.get(context) {
                Some(ctx) => Some(json!({
                    "callback": id,
                    "context": context,
                    "priority": ctx.get("priority").cloned().unwrap_or(Value::Null),
                    "source": "nros.toml",
                })),
                None => Some(json!({
                    "callback": id,
                    "context": context,
                    "priority": null,
                    "source": "source_metadata",
                })),
            }
        })
        .collect()
}

fn schema_remaps(value: Option<&Value>) -> Vec<Value> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            Value::Array(pair) if pair.len() == 2 => Some(json!({
                "from": pair[0].as_str().unwrap_or_default(),
                "to": pair[1].as_str().unwrap_or_default(),
            })),
            _ => None,
        })
        .collect()
}

/// Phase 211.E — reshape an intermediate executable entry from
/// [`build_executable_entry`] into the public `PlanExecutable` schema. The
/// intermediate already carries `id` / `name` / `namespace` / `cmd` / `args`
/// in their public shape; we only reshape `env` (pairs → `{name, value}`)
/// and append the `trace` block.
fn schema_executable(entry: &Value) -> Value {
    let id = entry
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("executable");
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("executable");
    let namespace = entry
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("/");
    let cmd = entry
        .get("cmd")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let args = entry
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    json!({
        "id": id,
        "name": name,
        "namespace": namespace,
        "cmd": cmd,
        "args": args,
        "env": schema_env(entry.get("env")),
        "trace": {
            "launch_record_entity": format!("record://{id}"),
        },
    })
}

/// Phase 211.E — reshape an `env` field from its intermediate `[[name, value],
/// …]` representation into the public schema's `[{"name": …, "value": …}, …]`.
/// Parallel to [`schema_remaps`]; always returns an array (empty when nothing
/// is declared) so deploy-stage consumers can iterate without a presence
/// check.
fn schema_env(value: Option<&Value>) -> Vec<Value> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            Value::Array(pair) if pair.len() == 2 => Some(json!({
                "name": pair[0].as_str().unwrap_or_default(),
                "value": pair[1].as_str().unwrap_or_default(),
            })),
            Value::Object(map) => {
                let name = map.get("name").or_else(|| map.get("key"))?.as_str()?;
                let value = map.get("value")?.as_str().unwrap_or_default();
                Some(json!({ "name": name, "value": value }))
            }
            _ => None,
        })
        .collect()
}

fn schema_entity(
    instance_id: &str,
    entity: &Value,
    callback_lookup: &CallbackIdLookup,
) -> Option<Value> {
    let role = entity.get("role").and_then(Value::as_str)?;
    let source_entity = entity
        .get("source_id")
        .and_then(Value::as_str)
        .unwrap_or("entity");
    let id = schema_entity_id(instance_id, entity, source_entity);
    let callback = schema_entity_callback(entity, callback_lookup);
    let mut trace = json!({
        "source_artifact": {
            "artifact": entity.get("source_artifact").and_then(Value::as_str).unwrap_or("source-metadata.json"),
            "line": null,
            "column": null,
        },
        "manifest_endpoint": null,
    });
    copy_json_field(&mut trace, entity, "declaration_slot");
    match role {
        "publisher" => Some(json!({
            "role": role,
            "id": id,
            "source_entity": source_entity,
            "resolved_name": entity.get("resolved_name").and_then(Value::as_str).unwrap_or(""),
            "interface": schema_interface(entity.get("type"))?,
            "qos": schema_qos(entity.get("qos")),
            "trace": trace,
        })),
        "subscriber" => Some(json!({
            "role": role,
            "id": id,
            "source_entity": source_entity,
            "callback": callback,
            "resolved_name": entity.get("resolved_name").and_then(Value::as_str).unwrap_or(""),
            "interface": schema_interface(entity.get("type"))?,
            "qos": schema_qos(entity.get("qos")),
            "trace": trace,
        })),
        "timer" => Some(json!({
            "role": "timer",
            "id": id,
            "source_entity": source_entity,
            "callback": callback,
            "period_ms": entity.get("period_ms").and_then(Value::as_u64).unwrap_or(0),
            "trace": trace,
        })),
        "service_server" | "action_server" => Some(json!({
            "role": role,
            "id": id,
            "source_entity": source_entity,
            "callback": callback,
            "resolved_name": entity.get("resolved_name").and_then(Value::as_str).unwrap_or(""),
            "interface": schema_interface(entity.get("type"))?,
            "qos": null,
            "trace": trace,
        })),
        "service_client" | "action_client" => Some(json!({
            "role": role,
            "id": id,
            "source_entity": source_entity,
            "resolved_name": entity.get("resolved_name").and_then(Value::as_str).unwrap_or(""),
            "interface": schema_interface(entity.get("type"))?,
            "qos": null,
            "trace": trace,
        })),
        _ => None,
    }
}

fn schema_node_id(instance_id: &str, node: &Value) -> String {
    let source_node = node.get("id").and_then(Value::as_str).unwrap_or("node");
    generated_plan_id(instance_id, "node", declaration_slot(node), source_node)
}

fn schema_entity_id(instance_id: &str, entity: &Value, source_entity: &str) -> String {
    generated_plan_id(
        instance_id,
        "entity",
        declaration_slot(entity),
        source_entity,
    )
}

fn schema_callback_id(instance_id: &str, callback: &Value, source_callback: &str) -> String {
    generated_plan_id(
        instance_id,
        "callback",
        declaration_slot(callback),
        source_callback,
    )
}

fn generated_plan_id(
    instance_id: &str,
    generated_prefix: &str,
    declaration_slot: Option<u64>,
    source_id: &str,
) -> String {
    match declaration_slot {
        Some(slot) => format!("{instance_id}/{generated_prefix}_{slot}"),
        None => format!("{instance_id}/{source_id}"),
    }
}

fn declaration_slot(value: &Value) -> Option<u64> {
    value.get("declaration_slot").and_then(Value::as_u64)
}

fn schema_entity_callback(entity: &Value, callback_lookup: &CallbackIdLookup) -> Option<String> {
    let source_callback = entity.get("callback").and_then(Value::as_str)?;
    Some(callback_lookup.resolve(
        source_callback,
        entity.get("callback_slot").and_then(Value::as_u64),
    ))
}

fn schema_interface(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::Object(map) => Some(json!({
            "package": map.get("package").and_then(Value::as_str).unwrap_or(""),
            "name": map.get("name").and_then(Value::as_str).unwrap_or(""),
            "kind": map.get("kind").and_then(Value::as_str).unwrap_or("message"),
        })),
        Value::String(raw) => {
            let (package, name) = raw.split_once('/').unwrap_or(("", raw));
            Some(json!({
                "package": package,
                "name": name,
                "kind": if name.starts_with("srv/") {
                    "service"
                } else if name.starts_with("action/") {
                    "action"
                } else {
                    "message"
                },
            }))
        }
        _ => None,
    }
}

fn schema_qos(value: Option<&Value>) -> Value {
    if let Some(value) = value.filter(|value| !value.is_null()) {
        return value.clone();
    }
    json!({
        "reliability": "system_default",
        "durability": "system_default",
        "history": "system_default",
        "depth": 0,
        "deadline_ms": null,
        "lifespan_ms": null,
        "liveliness": "system_default",
        "liveliness_lease_duration_ms": null,
        "extensions": {},
    })
}

/// Phase 211.H — the launch-parameter prefix ROS 2 uses to carry per-topic QoS
/// overrides (`qos_overrides.<topic>.<role>.<policy>`). These are split out of
/// the generic `parameters` table into `schema_qos_overrides`.
const QOS_OVERRIDE_PREFIX: &str = "qos_overrides.";

fn schema_parameters(default_node_id: &str, value: Option<&Value>) -> Vec<Value> {
    let Some(Value::Object(map)) = value else {
        return Vec::new();
    };
    map.iter()
        // `parameter_files` is metadata, not a param; `qos_overrides.*` are
        // lowered separately into the typed `qos_overrides` block (211.H).
        .filter(|(name, _)| {
            name.as_str() != "parameter_files" && !name.starts_with(QOS_OVERRIDE_PREFIX)
        })
        .map(|(name, value)| {
            json!({
                "node": default_node_id,
                "name": name,
                "value": schema_parameter_value(value),
                "source": {
                    "kind": "launch",
                    "artifact": "launch",
                },
            })
        })
        .collect()
}

/// Phase 211.H — lower `qos_overrides.<topic>.<role>.<policy>` launch params
/// into typed `{topic, role, policy, value}` entries. The param name carries
/// dots as separators but the topic itself contains `/` (not `.`), so the
/// trailing two dot-segments are `<role>.<policy>` and everything before them
/// is the topic — `rsplitn(3, '.')` recovers all three. Names that don't carry
/// both a role and a policy after the prefix are skipped (malformed → no
/// override rather than a panic).
fn schema_qos_overrides(value: Option<&Value>) -> Vec<Value> {
    let Some(Value::Object(map)) = value else {
        return Vec::new();
    };
    let mut out: Vec<Value> = map
        .iter()
        .filter_map(|(name, value)| {
            let rest = name.strip_prefix(QOS_OVERRIDE_PREFIX)?;
            // rsplitn(3, '.') → [policy, role, topic]
            let mut parts = rest.rsplitn(3, '.');
            let policy = parts.next()?;
            let role = parts.next()?;
            let topic = parts.next()?;
            if topic.is_empty() || role.is_empty() || policy.is_empty() {
                return None;
            }
            Some(json!({
                "topic": topic,
                "role": role,
                "policy": policy,
                "value": schema_parameter_value(value),
                "source": { "kind": "launch", "artifact": "launch" },
            }))
        })
        .collect();
    // Deterministic order (BTreeMap source is already sorted, but the
    // topic/role/policy decomposition can reorder) for byte-stable plans.
    out.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["topic"].as_str().unwrap_or("").to_string(),
                v["role"].as_str().unwrap_or("").to_string(),
                v["policy"].as_str().unwrap_or("").to_string(),
            )
        };
        key(a).cmp(&key(b))
    });
    out
}

fn schema_parameter_value(value: &Value) -> Value {
    match value {
        Value::Bool(_) | Value::Number(_) | Value::String(_) => value.clone(),
        Value::Array(items) => {
            if items.iter().all(Value::is_boolean)
                || items.iter().all(|v| v.as_i64().is_some())
                || items.iter().all(|v| v.as_f64().is_some())
                || items.iter().all(Value::is_string)
            {
                value.clone()
            } else {
                Value::String(value.to_string())
            }
        }
        _ => Value::String(value.to_string()),
    }
}

fn schema_interfaces(instances: &[Value]) -> Vec<Value> {
    let mut used: std::collections::BTreeMap<String, (Value, Vec<String>)> =
        std::collections::BTreeMap::new();
    for entity in instances
        .iter()
        .flat_map(|instance| instance.get("nodes").and_then(Value::as_array))
        .flatten()
        .flat_map(|node| node.get("entities").and_then(Value::as_array))
        .flatten()
    {
        let Some(interface) = entity.get("interface") else {
            continue;
        };
        let package = interface
            .get("package")
            .and_then(Value::as_str)
            .unwrap_or("");
        let name = interface.get("name").and_then(Value::as_str).unwrap_or("");
        let key = format!("{package}/{name}");
        used.entry(key)
            .or_insert_with(|| (interface.clone(), Vec::new()))
            .1
            .push(
                entity
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            );
    }
    used.into_iter()
        .map(|(id, (interface, used_by))| {
            json!({
                "id": id,
                "interface": interface,
                "used_by": used_by,
            })
        })
        .collect()
}

/// Phase 172.B — infer callback execution chains from the topic dataflow graph.
///
/// An edge `K1 -> K2` (over topic `T`) exists when `K1`'s instance publishes `T`
/// and `K2` is the subscriber callback bound to `T`. An instance's *producing*
/// callbacks (its subscriber + timer callbacks — the things that run and may in
/// turn publish) are the sources of edges out of that instance; the plan does
/// not record which specific callback publishes which topic, so every producing
/// callback of a publishing instance is linked to the downstream subscriber
/// (the inference's known coarseness — overridable by an explicit `[[chain]]`).
///
/// Connected dataflow subgraphs become chains: callbacks are topologically
/// ordered (head → tail) and `links` records the producing topic per edge.
/// Pure pub/sub-less or unconnected callbacks yield no chain.
fn infer_callback_chains(instances: &[Value]) -> Vec<Value> {
    use std::collections::BTreeSet;

    // Per instance: its producing callback ids + the topics it publishes.
    let mut producing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut publishes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // topic -> subscriber callback ids (consumers).
    let mut consumers: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for instance in instances {
        let iid = instance
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        for entity in instance
            .get("nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|node| node.get("entities").and_then(Value::as_array))
            .flatten()
        {
            let role = entity.get("role").and_then(Value::as_str).unwrap_or("");
            let topic = entity.get("resolved_name").and_then(Value::as_str);
            let callback = entity.get("callback").and_then(Value::as_str);
            match role {
                "publisher" => {
                    if let Some(t) = topic {
                        publishes
                            .entry(iid.clone())
                            .or_default()
                            .insert(t.to_string());
                    }
                }
                "subscriber" => {
                    if let Some(cb) = callback {
                        producing
                            .entry(iid.clone())
                            .or_default()
                            .push(cb.to_string());
                        if let Some(t) = topic {
                            consumers
                                .entry(t.to_string())
                                .or_default()
                                .push(cb.to_string());
                        }
                    }
                }
                "timer" => {
                    if let Some(cb) = callback {
                        producing
                            .entry(iid.clone())
                            .or_default()
                            .push(cb.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    // Edges (from_cb, to_cb, topic), de-duplicated and deterministically ordered.
    let mut edges: BTreeSet<(String, String, String)> = BTreeSet::new();
    for (iid, topics) in &publishes {
        let Some(srcs) = producing.get(iid) else {
            continue;
        };
        for topic in topics {
            let Some(dsts) = consumers.get(topic) else {
                continue;
            };
            for from in srcs {
                for to in dsts {
                    if from != to {
                        edges.insert((from.clone(), to.clone(), topic.clone()));
                    }
                }
            }
        }
    }
    if edges.is_empty() {
        return Vec::new();
    }

    // Union-find over callbacks that participate in an edge → weakly-connected
    // components, one chain each.
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    fn find(parent: &mut BTreeMap<String, String>, x: &str) -> String {
        let p = parent.get(x).cloned().unwrap_or_else(|| x.to_string());
        if p == x {
            return p;
        }
        let root = find(parent, &p);
        parent.insert(x.to_string(), root.clone());
        root
    }
    for (from, to, _) in &edges {
        parent.entry(from.clone()).or_insert_with(|| from.clone());
        parent.entry(to.clone()).or_insert_with(|| to.clone());
        let ra = find(&mut parent, from);
        let rb = find(&mut parent, to);
        if ra != rb {
            parent.insert(ra, rb);
        }
    }

    // Group edges by component root.
    let mut comp_edges: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for e in &edges {
        let root = find(&mut parent, &e.0);
        comp_edges.entry(root).or_default().push(e.clone());
    }

    let mut chains: Vec<Value> = Vec::new();
    for (_root, comp) in comp_edges {
        // Kahn topological order over this component.
        let mut indeg: BTreeMap<String, usize> = BTreeMap::new();
        let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut nodes: BTreeSet<String> = BTreeSet::new();
        for (from, to, _) in &comp {
            nodes.insert(from.clone());
            nodes.insert(to.clone());
            adj.entry(from.clone()).or_default().push(to.clone());
            *indeg.entry(to.clone()).or_insert(0) += 1;
            indeg.entry(from.clone()).or_insert(0);
        }
        let mut queue: std::collections::VecDeque<String> = nodes
            .iter()
            .filter(|n| indeg.get(*n).copied().unwrap_or(0) == 0)
            .cloned()
            .collect();
        let mut order: Vec<String> = Vec::new();
        while let Some(n) = queue.pop_front() {
            order.push(n.clone());
            if let Some(succ) = adj.get(&n) {
                for s in succ {
                    let d = indeg.get_mut(s).unwrap();
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(s.clone());
                    }
                }
            }
        }
        // A cycle leaves some nodes unvisited — append them deterministically so
        // the chain still lists every callback (links remain the source of truth).
        for n in &nodes {
            if !order.contains(n) {
                order.push(n.clone());
            }
        }
        let head = order.first().cloned().unwrap_or_default();
        let links: Vec<Value> = comp
            .iter()
            .map(|(from, to, topic)| json!({ "from": from, "to": to, "topic": topic }))
            .collect();
        chains.push(json!({
            "id": format!("chain/{head}"),
            "callbacks": order,
            "links": links,
            "inferred": true,
        }));
    }
    chains
}

/// Phase 172.C — derive callback groups from the 172.B chains. Each chain
/// becomes one `mutually_exclusive` group (its dataflow-coupled stages
/// serialize, preserving pipeline ordering + guarding shared state); each
/// callback that appears in no chain becomes its own `reentrant` group (no
/// coupling detected ⇒ concurrent-safe dispatch). Determinism: chain groups
/// emit in `chains` order (already id-sorted by component root), then
/// reentrant singletons in callback-id order. Overridable by an explicit
/// `[[group]]`.
fn infer_callback_groups(instances: &[Value], chains: &[Value]) -> Vec<Value> {
    use std::collections::BTreeSet;

    let mut grouped: BTreeSet<String> = BTreeSet::new();
    let mut groups: Vec<Value> = Vec::new();

    // One mutually-exclusive group per chain.
    for chain in chains {
        let cbs: Vec<String> = chain
            .get("callbacks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        for c in &cbs {
            grouped.insert(c.clone());
        }
        let head = cbs.first().cloned().unwrap_or_default();
        groups.push(json!({
            "id": format!("group/{head}"),
            "kind": "mutually_exclusive",
            "callbacks": cbs,
            "inferred": true,
        }));
    }

    // One reentrant singleton group per callback outside any chain.
    let mut singles: Vec<String> = Vec::new();
    for instance in instances {
        for cb in instance
            .get("callbacks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = cb.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !grouped.contains(id) {
                singles.push(id.to_string());
            }
        }
    }
    singles.sort();
    singles.dedup();
    for id in singles {
        groups.push(json!({
            "id": format!("group/{id}"),
            "kind": "reentrant",
            "callbacks": [id],
            "inferred": true,
        }));
    }

    groups
}

fn build_instances(
    record: &Value,
    metadata: &[JsonArtifact],
    workspace: &Workspace,
    overlays: &[Value],
    record_path: &Path,
) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let mut counts = HashMap::<(String, String), usize>::new();
    let mut exec_counts = HashMap::<String, usize>::new();
    let mut diagnostics = Vec::new();
    let mut instances = Vec::new();
    let mut executables = Vec::new();

    // Phase 211.B — index containers by canonical name → instance id so the
    // composable loop below can link each child to its parent. The canonical
    // key matches the parser's `target_container_name` shape: an absolute
    // path like `/my_container` (the parent's launch_name) for resolved
    // entries. We populate the map AS we mint each container instance.
    let mut container_id_by_launch_name: HashMap<String, String> = HashMap::new();

    for container in record_array(record, "container") {
        let package = string_field(container, &["package"]).unwrap_or_default();
        if package.is_empty() {
            continue;
        }
        let executable = string_field(container, &["executable"]).unwrap_or_default();
        let params = pairs_field(container, "params");
        let remaps = pairs_field(container, "remaps");
        let env = pairs_field(container, "env");
        let param_files = string_list_field(container, "params_files");
        let name = string_field(container, &["name"]);
        let namespace = string_field(container, &["namespace"]);
        let launch_name = names::node_fqn(namespace, name, executable);
        let inst = build_node_instance(
            NodeInstanceSpec {
                package,
                executable,
                name,
                namespace,
                params: &params,
                param_files: &param_files,
                remaps: &remaps,
                env: &env,
                domain_id: u32_field(container, &["domain_id", "domain"]),
                launch_kind: "container",
                container_id: None,
                machine: string_field(container, &["machine"]),
            },
            &mut PlanCtx {
                metadata,
                workspace,
                overlays,
                record_path,
                counts: &mut counts,
                diagnostics: &mut diagnostics,
            },
        );
        if let Some(id) = inst.get("id").and_then(Value::as_str) {
            container_id_by_launch_name.insert(launch_name.clone(), id.to_string());
            // Composable launches reference the container by FQN (e.g.
            // `/my_container`) on `target_container_name`; some launches use
            // the bare `name` instead. Store both forms so the lookup is
            // robust to either spelling.
            if let Some(name) = name {
                container_id_by_launch_name.insert(name.to_string(), id.to_string());
            }
        }
        instances.push(inst);
    }

    for node in record_array(record, "node") {
        let package = string_field(node, &["package"]).unwrap_or_default();
        if package.is_empty() {
            // Phase 211.E — a `<executable>` from the launch lands here.
            // `play_launch_parser` writes every `<executable cmd="…">` as a
            // `record.node` with `package=None`; the planner used to emit a
            // `missing-package` error, which made any launch carrying a
            // `<executable>` unplanable. Now they're surfaced as non-rmw
            // spawn entries the deploy stage runs alongside the rmw
            // `instances`.
            executables.push(build_executable_entry(node, &mut exec_counts));
            continue;
        }
        let executable = string_field(node, &["executable"]).unwrap_or_default();
        let params = pairs_field(node, "params");
        let remaps = pairs_field(node, "remaps");
        let env = pairs_field(node, "env");
        let param_files = string_list_field(node, "params_files");
        instances.push(build_node_instance(
            NodeInstanceSpec {
                package,
                executable,
                name: string_field(node, &["name"]),
                namespace: string_field(node, &["namespace"]),
                params: &params,
                param_files: &param_files,
                remaps: &remaps,
                env: &env,
                domain_id: u32_field(node, &["domain_id", "domain"]),
                launch_kind: "node",
                container_id: None,
                machine: string_field(node, &["machine"]),
            },
            &mut PlanCtx {
                metadata,
                workspace,
                overlays,
                record_path,
                counts: &mut counts,
                diagnostics: &mut diagnostics,
            },
        ));
    }

    for load_node in record_array(record, "load_node") {
        let package = string_field(load_node, &["package"]).unwrap_or_default();
        let plugin = string_field(load_node, &["plugin"]).unwrap_or_default();
        let executable = plugin.split("::").last().unwrap_or(plugin);
        let params = pairs_field(load_node, "params");
        let remaps = pairs_field(load_node, "remaps");
        let env = pairs_field(load_node, "env");
        // Phase 211.B — resolve the parent container's instance id from the
        // parser's `target_container_name`. Try the FQN as-is, the leading
        // slash stripped, and the trailing path segment — covers every form
        // we've seen on Autoware launches (parser writes the FQN).
        let target = string_field(load_node, &["target_container_name"]).unwrap_or("");
        let container_id = container_id_by_launch_name
            .get(target)
            .or_else(|| container_id_by_launch_name.get(target.trim_start_matches('/')))
            .or_else(|| {
                target
                    .rsplit('/')
                    .next()
                    .and_then(|tail| container_id_by_launch_name.get(tail))
            })
            .cloned();
        instances.push(build_node_instance(
            NodeInstanceSpec {
                package,
                executable,
                name: string_field(load_node, &["node_name"]),
                namespace: string_field(load_node, &["namespace"]),
                params: &params,
                param_files: &[],
                remaps: &remaps,
                env: &env,
                domain_id: u32_field(load_node, &["domain_id", "domain"]),
                launch_kind: "load_node",
                container_id: container_id.as_deref(),
                machine: string_field(load_node, &["machine"]),
            },
            &mut PlanCtx {
                metadata,
                workspace,
                overlays,
                record_path,
                counts: &mut counts,
                diagnostics: &mut diagnostics,
            },
        ));
    }

    (instances, executables, diagnostics)
}

/// Phase 211.E — build an intermediate executable entry from a `record.node`
/// whose `package` is missing (the parser's marker for `<executable>`).
/// Output shape is parallel to [`build_node_instance`]'s instance: a serde
/// JSON object the downstream [`schema_executable`] reshapes into the public
/// schema. `exec_counts` per-name bumps the synthesized id so multiple
/// `<executable name="…">` entries with the same name stay distinct.
fn build_executable_entry(node: &Value, exec_counts: &mut HashMap<String, usize>) -> Value {
    let raw_name = string_field(node, &["name", "exec_name"]).unwrap_or("executable");
    let name = raw_name.to_string();
    let sanitized = sanitize_id(raw_name);
    let index = {
        let entry = exec_counts.entry(sanitized.clone()).or_insert(0);
        let i = *entry;
        *entry += 1;
        i
    };
    let id = format!("executable.{sanitized}.{index}");
    let namespace = names::normalize_namespace(string_field(node, &["namespace"]));
    let cmd = string_list_field(node, "cmd");
    let args = string_list_field(node, "args");
    let env = pairs_field(node, "env");
    json!({
        "id": id,
        "name": name,
        "namespace": namespace,
        "cmd": cmd,
        "args": args,
        "env": env,
    })
}

/// Per-node inputs for [`build_node_instance`].
struct NodeInstanceSpec<'a> {
    package: &'a str,
    executable: &'a str,
    name: Option<&'a str>,
    namespace: Option<&'a str>,
    params: &'a [(String, String)],
    param_files: &'a [String],
    remaps: &'a [(String, String)],
    /// Environment variables flowing onto the spawned process. Sourced from
    /// the launch file's `<set_env>` / `<env>` elements via the parser
    /// (`record.node[*].env`); the planner threads them through verbatim so
    /// the deploy stage can hand them to the spawn / systemd / runtime
    /// equivalent. Phase 211.E.
    env: &'a [(String, String)],
    domain_id: Option<u32>,
    launch_kind: &'a str,
    /// Phase 211.B — when this instance is a `<composable_node>` child, the
    /// instance id of the parent `<node_container>` (resolved from the
    /// parser's `target_container_name`). `None` for plain `<node>` and
    /// for `<node_container>` itself.
    container_id: Option<&'a str>,
    /// Phase 211.F — `<node machine="…">` target host (parser `node.machine`).
    /// `None` for single-host launches. schema_instance lowers it onto the
    /// public `host_id` field.
    machine: Option<&'a str>,
}

/// Ambient state threaded through plan construction: read-only inputs
/// plus the two accumulators ([`counts`](Self::counts) for per-package
/// instance indices and [`diagnostics`](Self::diagnostics)).
struct PlanCtx<'a> {
    metadata: &'a [JsonArtifact],
    workspace: &'a Workspace,
    overlays: &'a [Value],
    record_path: &'a Path,
    counts: &'a mut HashMap<(String, String), usize>,
    diagnostics: &'a mut Vec<Value>,
}

fn build_node_instance(spec: NodeInstanceSpec<'_>, ctx: &mut PlanCtx<'_>) -> Value {
    let NodeInstanceSpec {
        package,
        executable,
        name,
        namespace,
        params,
        param_files,
        remaps,
        env,
        domain_id,
        launch_kind,
        container_id,
        machine,
    } = spec;
    let metadata = ctx.metadata;
    let workspace = ctx.workspace;
    let overlays = ctx.overlays;
    let record_path = ctx.record_path;

    let index = next_instance_index(ctx.counts, package, executable);
    let instance_id = format!(
        "{}.{}.{}",
        sanitize_id(package),
        sanitize_id(executable),
        index
    );
    let namespace = names::normalize_namespace(namespace);
    let source_metadata = find_source_metadata(metadata, package, executable);
    let effective_name =
        match effective_node_name(name, source_metadata.map(|artifact| &artifact.value)) {
            Ok(name) => name,
            Err(err) => {
                ctx.diagnostics.push(diagnostic(
                    "error",
                    "missing-effective-node-name",
                    err.message(package, executable),
                    Some(package),
                    Some(&instance_id),
                    None,
                    record_path,
                ));
                // Keep building enough structure to report all diagnostics from
                // this planning pass. The error above prevents plan emission.
                executable.to_string()
            }
        };
    let node_name = names::node_fqn(Some(&namespace), Some(&effective_name), &effective_name);
    // Phase 211.B — `<node_container>` typically spawns a stock binary
    // (e.g. rclcpp_components::component_container) that isn't a nros
    // component and so has no source_metadata. The composable children
    // each carry their own metadata; the container itself doesn't need
    // any. Suppress the missing-source-metadata diagnostic for containers.
    if source_metadata.is_none() && launch_kind != "container" {
        ctx.diagnostics.push(diagnostic(
            "error",
            "missing-source-metadata",
            format!("missing source metadata for {package}/{executable}"),
            Some(package),
            Some(&instance_id),
            None,
            record_path,
        ));
    }

    let package_nros = workspace
        .package_nros_toml(package)
        .and_then(|path| load_toml_values(&[path]).ok())
        .and_then(|mut values| values.pop());
    let parameters = effective_parameters(ParameterInputs {
        source_metadata: source_metadata.map(|artifact| &artifact.value),
        package_nros: package_nros.as_ref(),
        launch_params: params,
        param_files,
        overlays,
    });
    let entities = source_metadata
        .map(|artifact| {
            source_entities(
                &artifact.value,
                &artifact.path,
                &namespace,
                &effective_name,
                remaps,
            )
        })
        .unwrap_or_default();
    let nodes = source_metadata
        .map(|artifact| source_nodes(&artifact.value, &namespace, &effective_name, domain_id))
        .unwrap_or_else(|| {
            let mut node = json!({
                "id": "node",
                "resolved_name": node_name,
                "namespace": namespace,
            });
            if let Some(domain_id) = domain_id {
                node.as_object_mut()
                    .expect("fallback source node is an object")
                    .insert("domain_id".to_string(), json!(domain_id));
            }
            vec![node]
        });
    let callbacks = source_metadata
        .map(|artifact| source_callbacks(&artifact.value))
        .unwrap_or_default();
    if let Some(artifact) = source_metadata {
        ctx.diagnostics.extend(check_source_metadata_links(
            &artifact.value,
            &artifact.path,
            package,
            &instance_id,
        ));
    }

    json!({
        "id": instance_id,
        "telemetry_id": format!("{package}::{executable}#{index}"),
        "package": package,
        "executable": executable,
        "launch_kind": launch_kind,
        // Phase 211.B — `container_id` is None for plain `<node>` and for
        // `<node_container>` itself; Some for `<composable_node>` children.
        // schema_instance reshapes this onto the public `container_id`
        // field (skip_serializing_if = "Option::is_none").
        "container_id": container_id,
        // Phase 211.F — raw machine= target; schema_instance lowers it onto
        // the public `host_id` field (skip_serializing_if = "Option::is_none").
        "machine": machine,
        "node_name": node_name,
        "namespace": namespace,
        "domain_id": domain_id,
        "remaps": remaps,
        "parameters": parameters,
        // Forward raw pairs (matches `remaps` shape); `schema_env` reshapes
        // them into the public `{name, value}` schema. Phase 211.E.
        "env": env,
        "source_metadata": source_metadata.map(|artifact| artifact.path.to_string_lossy().to_string()),
        "nodes": nodes,
        "entities": entities,
        "callbacks": callbacks,
    })
}

enum EffectiveNodeNameError {
    MissingLaunchAndSourceDefault,
}

impl EffectiveNodeNameError {
    fn message(&self, package: &str, executable: &str) -> String {
        match self {
            Self::MissingLaunchAndSourceDefault => format!(
                "launch node {package}/{executable} omits name= and source metadata does not declare a static default node name"
            ),
        }
    }
}

fn effective_node_name(
    launch_name: Option<&str>,
    source_metadata: Option<&Value>,
) -> Result<String, EffectiveNodeNameError> {
    if let Some(name) = launch_name.filter(|name| !name.trim().is_empty()) {
        return Ok(name.to_string());
    }
    if let Some(default_name) = source_metadata.and_then(source_default_node_name) {
        return Ok(default_name.to_string());
    }
    // Dynamic source names still require launch `name=` so the workspace
    // graph can be audited at build time.
    Err(EffectiveNodeNameError::MissingLaunchAndSourceDefault)
}

fn source_default_node_name(metadata: &Value) -> Option<&str> {
    let nodes = metadata.get("nodes").and_then(Value::as_array)?;
    if nodes.len() != 1 {
        return None;
    }
    if let Some(name) = nodes[0]
        .get("source_default_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return Some(name);
    }
    let name = source_name_value(nodes[0].get("unresolved_name")).trim();
    if name.is_empty() { None } else { Some(name) }
}

fn check_effective_graph_node_names(instances: &[Value], record_path: &Path) -> Vec<Value> {
    let mut diagnostics = Vec::new();
    let mut seen = HashMap::<(u32, String, String), (String, String)>::new();
    for instance in instances {
        let instance_id = instance.get("id").and_then(Value::as_str).unwrap_or("");
        let package = instance.get("package").and_then(Value::as_str);
        let Some(nodes) = instance.get("nodes").and_then(Value::as_array) else {
            continue;
        };
        for node in nodes {
            let resolved = node
                .get("resolved_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let Some((namespace, name)) = graph_name_parts(resolved) else {
                continue;
            };
            let domain_id = node
                .get("domain_id")
                .and_then(Value::as_u64)
                .or_else(|| instance.get("domain_id").and_then(Value::as_u64))
                .unwrap_or(0) as u32;
            let key = (domain_id, namespace.to_string(), name.to_string());
            if let Some((first_instance, first_resolved)) = seen.get(&key) {
                diagnostics.push(diagnostic(
                    "error",
                    "duplicate-effective-node-name",
                    format!(
                        "duplicate ROS node name {resolved} in domain {domain_id}: {first_instance} already planned {first_resolved}; {instance_id} plans {resolved}"
                    ),
                    package,
                    Some(instance_id),
                    Some(resolved),
                    record_path,
                ));
            } else {
                seen.insert(key, (instance_id.to_string(), resolved.to_string()));
            }
        }
    }
    diagnostics
}

fn graph_name_parts(resolved: &str) -> Option<(&str, &str)> {
    let resolved = resolved.trim();
    if resolved.is_empty() || resolved == "/" {
        return None;
    }
    let (namespace, name) = resolved.rsplit_once('/').unwrap_or(("/", resolved));
    if name.is_empty() {
        return None;
    }
    let namespace = if namespace.is_empty() { "/" } else { namespace };
    Some((namespace, name))
}

fn check_source_metadata_links(
    metadata: &Value,
    path: &Path,
    package: &str,
    instance_id: &str,
) -> Vec<Value> {
    let entity_ids = source_entity_ids(metadata);
    let callback_ids = source_callback_ids(metadata);
    let mut diagnostics = Vec::new();

    if let Some(callbacks) = metadata.get("callbacks").and_then(Value::as_array) {
        for callback in callbacks {
            let callback_id = callback.get("id").and_then(Value::as_str).unwrap_or("");
            let Some(effects) = callback.get("effects").and_then(Value::as_array) else {
                continue;
            };
            for effect in effects {
                let entity_id = effect.get("entity").and_then(Value::as_str).unwrap_or("");
                if !entity_id.is_empty() && !entity_ids.contains(entity_id) {
                    diagnostics.push(diagnostic(
                        "error",
                        "callback-effect-unknown-entity",
                        format!(
                            "callback {callback_id} effect references unknown entity {entity_id}"
                        ),
                        Some(package),
                        Some(instance_id),
                        Some(entity_id),
                        path,
                    ));
                }
            }
        }
    }

    for (entity_id, callback_id) in source_entity_callback_refs(metadata) {
        if !callback_id.is_empty() && !callback_ids.contains(callback_id.as_str()) {
            diagnostics.push(diagnostic(
                "error",
                "entity-callback-missing",
                format!("entity {entity_id} references missing callback {callback_id}"),
                Some(package),
                Some(instance_id),
                Some(&entity_id),
                path,
            ));
        }
    }

    diagnostics
}

fn source_entity_ids(metadata: &Value) -> HashSet<&str> {
    let mut ids = HashSet::new();
    collect_source_entity_ids(metadata.get("entities"), &mut ids);
    collect_source_entity_ids(metadata.get("publishers"), &mut ids);
    collect_source_entity_ids(metadata.get("subscriptions"), &mut ids);
    collect_source_entity_ids(metadata.get("subscribers"), &mut ids);
    collect_source_entity_ids(metadata.get("services"), &mut ids);
    collect_source_entity_ids(metadata.get("clients"), &mut ids);
    collect_source_entity_ids(metadata.get("actions"), &mut ids);
    collect_source_entity_ids(metadata.get("parameters"), &mut ids);
    if let Some(nodes) = metadata.get("nodes").and_then(Value::as_array) {
        for node in nodes {
            collect_source_entity_ids(node.get("publishers"), &mut ids);
            collect_source_entity_ids(node.get("subscribers"), &mut ids);
            collect_source_entity_ids(node.get("timers"), &mut ids);
            collect_source_entity_ids(node.get("services"), &mut ids);
            collect_source_entity_ids(node.get("actions"), &mut ids);
            collect_source_entity_ids(node.get("parameters"), &mut ids);
        }
    }
    ids
}

fn collect_source_entity_ids<'a>(value: Option<&'a Value>, ids: &mut HashSet<&'a str>) {
    let Some(items) = value.and_then(Value::as_array) else {
        return;
    };
    for item in items {
        if let Some(id) = item
            .get("id")
            .or_else(|| item.get("entity"))
            .and_then(Value::as_str)
        {
            ids.insert(id);
        }
    }
}

fn source_callback_ids(metadata: &Value) -> HashSet<&str> {
    metadata
        .get("callbacks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|callback| callback.get("id").and_then(Value::as_str))
        .collect()
}

fn source_entity_callback_refs(metadata: &Value) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    collect_source_entity_callback_refs(metadata.get("entities"), &mut refs);
    collect_source_entity_callback_refs(metadata.get("subscriptions"), &mut refs);
    collect_source_entity_callback_refs(metadata.get("subscribers"), &mut refs);
    collect_source_entity_callback_refs(metadata.get("services"), &mut refs);
    collect_source_entity_callback_refs(metadata.get("actions"), &mut refs);
    if let Some(nodes) = metadata.get("nodes").and_then(Value::as_array) {
        for node in nodes {
            collect_source_entity_callback_refs(node.get("subscribers"), &mut refs);
            collect_source_entity_callback_refs(node.get("timers"), &mut refs);
            collect_source_entity_callback_refs(node.get("services"), &mut refs);
            collect_source_entity_callback_refs(node.get("actions"), &mut refs);
        }
    }
    refs
}

fn collect_source_entity_callback_refs(value: Option<&Value>, refs: &mut Vec<(String, String)>) {
    let Some(items) = value.and_then(Value::as_array) else {
        return;
    };
    for item in items {
        let entity_id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        for key in [
            "callback",
            "goal_callback",
            "cancel_callback",
            "accepted_callback",
        ] {
            let Some(callback_id) = item.get(key).and_then(Value::as_str) else {
                continue;
            };
            refs.push((entity_id.clone(), callback_id.to_string()));
        }
    }
}

fn source_callbacks(metadata: &Value) -> Vec<Value> {
    metadata
        .get("callbacks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn source_nodes(
    metadata: &Value,
    launch_namespace: &str,
    launch_node_name: &str,
    domain_id: Option<u32>,
) -> Vec<Value> {
    let Some(nodes) = metadata.get("nodes").and_then(Value::as_array) else {
        let mut node = json!({
            "id": "node",
            "resolved_name": names::node_fqn(Some(launch_namespace), Some(launch_node_name), launch_node_name),
            "namespace": launch_namespace,
        });
        if let Some(domain_id) = domain_id {
            node.as_object_mut()
                .expect("default source node is an object")
                .insert("domain_id".to_string(), json!(domain_id));
        }
        return vec![node];
    };
    let single_node = nodes.len() == 1;
    nodes
        .iter()
        .map(|node| {
            let source_node = node.get("id").and_then(Value::as_str).unwrap_or("node");
            let metadata_namespace = node
                .get("namespace")
                .and_then(Value::as_str)
                .unwrap_or(launch_namespace);
            let source_name = source_name_value(node.get("unresolved_name"));
            let resolved_name = if single_node {
                names::node_fqn(
                    Some(launch_namespace),
                    Some(launch_node_name),
                    launch_node_name,
                )
            } else {
                names::node_fqn(Some(metadata_namespace), Some(source_name), source_node)
            };
            let namespace = node_namespace(&resolved_name);
            let mut out = json!({
                "id": source_node,
                "resolved_name": resolved_name,
                "namespace": namespace,
            });
            if let Some(domain_id) = domain_id {
                out.as_object_mut()
                    .expect("source node is an object")
                    .insert("domain_id".to_string(), json!(domain_id));
            }
            copy_json_field(&mut out, node, "source_default_name");
            copy_json_field(&mut out, node, "declaration_slot");
            copy_json_field(&mut out, node, "source");
            out
        })
        .collect()
}

fn node_namespace(resolved_name: &str) -> String {
    let Some((namespace, _)) = resolved_name.rsplit_once('/') else {
        return "/".to_string();
    };
    if namespace.is_empty() {
        "/".to_string()
    } else {
        namespace.to_string()
    }
}

fn source_entities(
    metadata: &Value,
    path: &Path,
    namespace: &str,
    node_name: &str,
    remaps: &[(String, String)],
) -> Vec<Value> {
    let mut out = Vec::new();
    collect_schema_nodes(
        metadata.get("nodes"),
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    collect_entity_array(
        metadata.get("entities"),
        "entity",
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    collect_entity_array(
        metadata.get("publishers"),
        "publisher",
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    collect_entity_array(
        metadata.get("subscriptions"),
        "subscriber",
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    collect_entity_array(
        metadata.get("subscribers"),
        "subscriber",
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    collect_entity_array(
        metadata.get("services"),
        "service_server",
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    collect_entity_array(
        metadata.get("clients"),
        "service_client",
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    collect_entity_array(
        metadata.get("actions"),
        "action",
        path,
        namespace,
        node_name,
        remaps,
        &mut out,
    );
    out
}

fn collect_schema_nodes(
    value: Option<&Value>,
    path: &Path,
    namespace: &str,
    node_name: &str,
    remaps: &[(String, String)],
    out: &mut Vec<Value>,
) {
    let Some(Value::Array(nodes)) = value else {
        return;
    };
    let single_node = nodes.len() == 1;
    for node in nodes {
        let source_node = node.get("id").and_then(Value::as_str).unwrap_or("node");
        let metadata_namespace = node
            .get("namespace")
            .and_then(Value::as_str)
            .unwrap_or(namespace);
        let metadata_node_name = if single_node {
            node_name
        } else {
            source_name_value(node.get("unresolved_name"))
        };
        collect_schema_endpoint_array(
            node.get("publishers"),
            "publisher",
            "unresolved_topic",
            path,
            source_node,
            metadata_namespace,
            metadata_node_name,
            remaps,
            out,
        );
        collect_schema_endpoint_array(
            node.get("subscribers"),
            "subscriber",
            "unresolved_topic",
            path,
            source_node,
            metadata_namespace,
            metadata_node_name,
            remaps,
            out,
        );
        collect_schema_endpoint_array(
            node.get("services"),
            "service_server",
            "unresolved_name",
            path,
            source_node,
            metadata_namespace,
            metadata_node_name,
            remaps,
            out,
        );
        collect_schema_endpoint_array(
            node.get("actions"),
            "action_server",
            "unresolved_name",
            path,
            source_node,
            metadata_namespace,
            metadata_node_name,
            remaps,
            out,
        );
        collect_schema_timer_array(node.get("timers"), path, source_node, out);
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_schema_endpoint_array(
    value: Option<&Value>,
    role: &str,
    name_key: &str,
    path: &Path,
    source_node: &str,
    namespace: &str,
    node_name: &str,
    remaps: &[(String, String)],
    out: &mut Vec<Value>,
) {
    let Some(Value::Array(items)) = value else {
        return;
    };
    for item in items {
        let source_name = source_name_value(item.get(name_key));
        let resolved = names::resolve_entity_name(namespace, node_name, source_name, remaps);
        out.push(json!({
            "source_artifact": path,
            "source_node": source_node,
            "source_id": item.get("id"),
            "declaration_slot": item.get("declaration_slot"),
            "role": role,
            "source_name": resolved.source,
            "source_name_kind": source_name_kind(item.get(name_key)),
            "resolved_name": resolved.resolved,
            "remapped_from": resolved.remapped_from,
            "type": item.get("interface"),
            "qos": item.get("qos"),
            "callback": item.get("callback")
                .or_else(|| item.get("goal_callback")),
            "callback_slot": item.get("callback_slot")
                .or_else(|| item.get("goal_callback_slot")),
        }));
    }
}

fn collect_schema_timer_array(
    value: Option<&Value>,
    path: &Path,
    source_node: &str,
    out: &mut Vec<Value>,
) {
    let Some(Value::Array(items)) = value else {
        return;
    };
    for item in items {
        out.push(json!({
            "source_artifact": path,
            "source_node": source_node,
            "source_id": item.get("id"),
            "declaration_slot": item.get("declaration_slot"),
            "role": "timer",
            "period_ms": item.get("period_ms"),
            "callback": item.get("callback"),
            "callback_slot": item.get("callback_slot"),
        }));
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_entity_array(
    value: Option<&Value>,
    default_role: &str,
    path: &Path,
    namespace: &str,
    node_name: &str,
    remaps: &[(String, String)],
    out: &mut Vec<Value>,
) {
    let Some(Value::Array(items)) = value else {
        return;
    };
    for item in items {
        let role = item
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or(default_role);
        let source_name = item
            .get("name")
            .or_else(|| item.get("topic"))
            .or_else(|| item.get("service"))
            .or_else(|| item.get("action"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let resolved = names::resolve_entity_name(namespace, node_name, source_name, remaps);
        out.push(json!({
            "source_artifact": path,
            "source_node": "node",
            "source_id": item.get("id"),
            "declaration_slot": item.get("declaration_slot"),
            "role": normalize_role(role),
            "source_name": resolved.source,
            "source_name_kind": infer_source_name_kind(source_name),
            "resolved_name": resolved.resolved,
            "remapped_from": resolved.remapped_from,
            "type": item.get("type")
                .or_else(|| item.get("interface_type"))
                .or_else(|| item.get("message_type")),
        }));
    }
}

fn source_name_value(value: Option<&Value>) -> &str {
    match value {
        Some(Value::String(name)) => name,
        Some(Value::Object(map)) => map.get("value").and_then(Value::as_str).unwrap_or(""),
        _ => "",
    }
}

fn source_name_kind(value: Option<&Value>) -> &str {
    match value {
        Some(Value::Object(map)) => map
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_else(|| infer_source_name_kind(source_name_value(value))),
        Some(Value::String(name)) => infer_source_name_kind(name),
        _ => "relative",
    }
}

fn infer_source_name_kind(name: &str) -> &str {
    if name == "~" || name.starts_with("~/") {
        "private"
    } else if name.starts_with('/') {
        "absolute"
    } else {
        "relative"
    }
}

fn check_manifest_endpoints(
    instances: &[Value],
    manifests: &[ManifestArtifact],
    metadata: &[JsonArtifact],
    record_path: &Path,
) -> Vec<Value> {
    let mut diagnostics = Vec::new();
    if manifests.is_empty() {
        diagnostics.push(diagnostic(
            "warning",
            "missing-launch-manifest",
            "no ROS launch manifest files were loaded",
            None,
            None,
            None,
            record_path,
        ));
        return diagnostics;
    }
    let requirements = endpoint_requirements(manifests);
    for requirement in &requirements {
        if !entity_matches_requirement(instances, requirement) {
            diagnostics.push(diagnostic(
                "error",
                "manifest-endpoint-unmatched",
                format!(
                    "manifest endpoint did not match source metadata: role={} name={} type={}",
                    requirement
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("?"),
                    requirement
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?"),
                    requirement
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                ),
                None,
                None,
                Some(&artifact_list(metadata)),
                requirement
                    .get("source_artifact")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .as_deref()
                    .unwrap_or(record_path),
            ));
        }
    }
    diagnostics.extend(check_metadata_entities_in_manifest(
        instances,
        &requirements,
        record_path,
    ));
    diagnostics
}

fn check_metadata_entities_in_manifest(
    instances: &[Value],
    requirements: &[Value],
    record_path: &Path,
) -> Vec<Value> {
    let mut diagnostics = Vec::new();
    for instance in instances {
        let package = instance.get("package").and_then(Value::as_str);
        let instance_id = instance.get("id").and_then(Value::as_str);
        let Some(entities) = instance.get("entities").and_then(Value::as_array) else {
            continue;
        };
        for entity in entities {
            let role = entity.get("role").and_then(Value::as_str).unwrap_or("");
            if !is_manifest_endpoint_role(role) {
                continue;
            }
            if requirements
                .iter()
                .any(|requirement| entity_matches_single_requirement(instance, entity, requirement))
            {
                continue;
            }
            diagnostics.push(diagnostic(
                "error",
                "metadata-entity-unmatched",
                format!(
                    "source metadata entity is not covered by launch manifest: role={} name={} type={}",
                    role,
                    entity
                        .get("resolved_name")
                        .and_then(Value::as_str)
                        .unwrap_or("?"),
                    entity_type_display(entity)
                ),
                package,
                instance_id,
                entity.get("source_id").and_then(Value::as_str),
                entity
                    .get("source_artifact")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .as_deref()
                    .unwrap_or(record_path),
            ));
        }
    }
    diagnostics
}

fn is_manifest_endpoint_role(role: &str) -> bool {
    matches!(
        role,
        "publisher"
            | "subscriber"
            | "service_server"
            | "service_client"
            | "action_server"
            | "action_client"
    )
}

fn entity_matches_requirement(instances: &[Value], requirement: &Value) -> bool {
    instances
        .iter()
        .filter(|instance| requirement_node_matches(instance, requirement))
        .any(|instance| {
            instance
                .get("entities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|entity| entity_matches_single_requirement(instance, entity, requirement))
        })
}

fn entity_matches_single_requirement(
    instance: &Value,
    entity: &Value,
    requirement: &Value,
) -> bool {
    if !requirement_node_matches(instance, requirement) {
        return false;
    }
    let role = requirement
        .get("role")
        .and_then(Value::as_str)
        .map(normalize_role)
        .unwrap_or_default();
    let name = requirement
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let interface_type = requirement.get("type").and_then(Value::as_str);
    entity.get("role").and_then(Value::as_str) == Some(role.as_str())
        && endpoint_name_matches(entity, name)
        && interface_type.is_none_or(|ty| entity_type_matches(entity, ty))
}

fn requirement_node_matches(instance: &Value, requirement: &Value) -> bool {
    let Some(required_node) = requirement.get("node").and_then(Value::as_str) else {
        return true;
    };
    let Some(instance_node) = instance.get("node_name").and_then(Value::as_str) else {
        return false;
    };
    instance_node == required_node
        || instance_node.trim_start_matches('/') == required_node.trim_start_matches('/')
}

fn endpoint_name_matches(entity: &Value, name: &str) -> bool {
    let Some(resolved) = entity.get("resolved_name").and_then(Value::as_str) else {
        return false;
    };
    resolved == name || resolved.trim_start_matches('/') == name.trim_start_matches('/')
}

fn entity_type_matches(entity: &Value, interface_type: &str) -> bool {
    let Some(ty) = entity.get("type") else {
        return false;
    };
    match ty {
        Value::String(s) => s == interface_type,
        Value::Object(map) => {
            let package = map.get("package").and_then(Value::as_str).unwrap_or("");
            let name = map.get("name").and_then(Value::as_str).unwrap_or("");
            format!("{package}/{name}") == interface_type
                || format!("{package}::{name}") == interface_type
        }
        _ => false,
    }
}

fn entity_type_display(entity: &Value) -> String {
    match entity.get("type") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(map)) => {
            let package = map.get("package").and_then(Value::as_str).unwrap_or("");
            let name = map.get("name").and_then(Value::as_str).unwrap_or("");
            format!("{package}/{name}")
        }
        _ => "?".to_string(),
    }
}

fn find_source_metadata<'a>(
    metadata: &'a [JsonArtifact],
    package: &str,
    executable: &str,
) -> Option<&'a JsonArtifact> {
    metadata
        .iter()
        .find(|artifact| metadata_matches(&artifact.value, package, executable))
}

fn metadata_matches(value: &Value, package: &str, executable: &str) -> bool {
    let package_match = string_field(value, &["package", "package_name"])
        .is_none_or(|candidate| candidate == package);
    let executable_match = string_field(value, &["executable", "executable_name", "component"])
        .is_none_or(|candidate| candidate == executable);
    package_match && executable_match
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

fn u32_field(value: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .and_then(|value| u32::try_from(value).ok())
}

fn copy_json_field(out: &mut Value, source: &Value, key: &str) {
    if let Some(value) = source.get(key) {
        if value.is_null() {
            return;
        }
        out.as_object_mut()
            .expect("target JSON value is an object")
            .insert(key.to_string(), value.clone());
    }
}

fn pairs_field(value: &Value, key: &str) -> Vec<(String, String)> {
    match value.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::Array(pair) if pair.len() == 2 => Some((
                    pair[0].as_str().unwrap_or_default().to_string(),
                    pair[1].as_str().unwrap_or_default().to_string(),
                )),
                Value::Object(map) => {
                    let key = map
                        .get("name")
                        .or_else(|| map.get("from"))
                        .or_else(|| map.get("key"))
                        .and_then(Value::as_str)?;
                    let value = map
                        .get("value")
                        .or_else(|| map.get("to"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    Some((key.to_string(), value.to_string()))
                }
                _ => None,
            })
            .collect(),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(key, value)| (key.clone(), scalar_to_string(value)))
            .collect(),
        _ => Vec::new(),
    }
}

fn string_list_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        other => other.to_string(),
    }
}

fn next_instance_index(
    counts: &mut HashMap<(String, String), usize>,
    package: &str,
    executable: &str,
) -> usize {
    let key = (package.to_string(), executable.to_string());
    let index = *counts.get(&key).unwrap_or(&0);
    counts.insert(key, index + 1);
    index
}

fn artifact_list(artifacts: &[JsonArtifact]) -> String {
    artifacts
        .iter()
        .map(|artifact| artifact.path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn diagnostic(
    severity: &str,
    code: &str,
    message: impl Into<String>,
    package: Option<&str>,
    instance: Option<&str>,
    entity: Option<&str>,
    artifact: &Path,
) -> Value {
    let mut object = Map::new();
    object.insert("severity".to_string(), Value::String(severity.to_string()));
    object.insert("code".to_string(), Value::String(code.to_string()));
    object.insert("message".to_string(), Value::String(message.into()));
    object.insert(
        "source_artifact".to_string(),
        Value::String(artifact.display().to_string()),
    );
    if let Some(package) = package {
        object.insert("package".to_string(), Value::String(package.to_string()));
    }
    if let Some(instance) = instance {
        object.insert("instance".to_string(), Value::String(instance.to_string()));
    }
    if let Some(entity) = entity {
        object.insert("entity".to_string(), Value::String(entity.to_string()));
    }
    Value::Object(object)
}

fn diagnostic_summary(diag: &Value) -> String {
    let code = diag.get("code").and_then(Value::as_str).unwrap_or("error");
    let message = diag.get("message").and_then(Value::as_str).unwrap_or("");
    let artifact = diag
        .get("source_artifact")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut scope = Vec::new();
    for key in ["package", "instance", "entity"] {
        if let Some(value) = diag.get(key).and_then(Value::as_str) {
            scope.push(format!("{key}={value}"));
        }
    }
    if scope.is_empty() {
        format!("{code}: {message} ({artifact})")
    } else {
        format!("{code}: {message} [{}] ({artifact})", scope.join(" "))
    }
}

fn normalize_role(role: &str) -> String {
    match role {
        "pub" | "publisher" => "publisher",
        "sub" | "subscriber" | "subscription" => "subscriber",
        "srv" | "server" | "service_server" => "service_server",
        "cli" | "client" | "service_client" => "service_client",
        "action_server" => "action_server",
        "action_client" => "action_client",
        other => other,
    }
    .to_string()
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_launch_args_in_ros_and_shell_forms() {
        let args = vec!["robot:=alpha".to_string(), "debug=true".to_string()];
        let parsed = parse_launch_args(&args).unwrap();
        assert_eq!(parsed["robot"], "alpha");
        assert_eq!(parsed["debug"], "true");
    }

    #[test]
    fn assigns_distinct_instance_indices() {
        let mut counts = HashMap::new();
        assert_eq!(next_instance_index(&mut counts, "pkg", "talker"), 0);
        assert_eq!(next_instance_index(&mut counts, "pkg", "talker"), 1);
    }

    #[test]
    fn plan_system_rejects_duplicate_effective_node_names() {
        let root = temp_workspace("nros-plan-duplicate-effective-node-name");
        let err = plan_with_record_and_metadata(
            &root,
            r#"{
  "node": [
    {"package": "demo_pkg", "executable": "talker", "name": "worker", "namespace": "/robot"},
    {"package": "demo_pkg", "executable": "talker", "name": "worker", "namespace": "/robot"}
  ]
}"#,
            &basic_talker_metadata("talker"),
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("duplicate-effective-node-name"), "{err}");
        assert!(err.contains("/robot/worker"), "{err}");
    }

    #[test]
    fn plan_system_allows_same_effective_node_name_in_different_namespaces() {
        let root = temp_workspace("nros-plan-same-node-name-different-ns");
        let output = plan_with_record_and_metadata(
            &root,
            r#"{
  "node": [
    {"package": "demo_pkg", "executable": "talker", "name": "worker", "namespace": "/robot_a"},
    {"package": "demo_pkg", "executable": "talker", "name": "worker", "namespace": "/robot_b"}
  ]
}"#,
            &basic_talker_metadata("talker"),
        )
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan).unwrap();
    }

    #[test]
    fn plan_system_allows_same_effective_node_name_in_different_domains() {
        let root = temp_workspace("nros-plan-same-node-name-different-domain");
        let output = plan_with_record_and_metadata(
            &root,
            r#"{
  "node": [
    {"package": "demo_pkg", "executable": "talker", "name": "worker", "namespace": "/robot", "domain_id": 0},
    {"package": "demo_pkg", "executable": "talker", "name": "worker", "namespace": "/robot", "domain_id": 7}
  ]
}"#,
            &basic_talker_metadata("talker"),
        )
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();
        assert_eq!(plan["instances"][0]["nodes"][0]["domain_id"], 0);
        assert_eq!(plan["instances"][1]["nodes"][0]["domain_id"], 7);
    }

    #[test]
    fn plan_system_allows_explicit_distinct_launch_node_names() {
        let root = temp_workspace("nros-plan-explicit-distinct-node-names");
        let output = plan_with_record_and_metadata(
            &root,
            r#"{
  "node": [
    {"package": "demo_pkg", "executable": "talker", "name": "talker_a", "namespace": "/"},
    {"package": "demo_pkg", "executable": "talker", "name": "talker_b", "namespace": "/"}
  ]
}"#,
            &basic_talker_metadata("talker"),
        )
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan).unwrap();
    }

    #[test]
    fn plan_system_uses_static_source_default_when_launch_name_is_missing() {
        let root = temp_workspace("nros-plan-source-default-node-name");
        let output = plan_with_record_and_metadata(
            &root,
            r#"{"node": [{"package": "demo_pkg", "executable": "talker", "namespace": "/"}]}"#,
            &basic_talker_metadata("source_talker"),
        )
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        assert_eq!(
            plan["instances"][0]["nodes"][0]["resolved_name"],
            "/source_talker"
        );
    }

    #[test]
    fn plan_system_rejects_missing_launch_name_without_source_default() {
        let root = temp_workspace("nros-plan-missing-effective-node-name");
        let err = plan_with_record_and_metadata(
            &root,
            r#"{"node": [{"package": "demo_pkg", "executable": "talker", "namespace": "/"}]}"#,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "nodes": [],
  "callbacks": [],
  "parameters": []
}"#,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("missing-effective-node-name"), "{err}");
        assert!(err.contains("omits name="), "{err}");
    }

    #[test]
    fn schema_build_json_defaults_when_no_overlay() {
        // No `[build]` / `[[transport]]` ⇒ pre-173.5 defaults, empty
        // transports — keeps existing plans byte-identical.
        let build = schema_build_json(&[], None);
        assert_eq!(build["board"], "native");
        assert_eq!(build["rmw"], "zenoh");
        assert_eq!(build["target"], "x86_64-unknown-linux-gnu");
        assert_eq!(build["transports"].as_array().unwrap().len(), 0);
        // Round-trips through the typed schema.
        serde_json::from_value::<PlanBuildOptions>(build).unwrap();
    }

    #[test]
    fn schema_build_json_reads_build_and_transports_from_overlay() {
        // Simulates an nros.toml parsed to JSON: `[build]` table +
        // `[[transport]]` (array key `transport`).
        let overlay = json!({
            "build": { "board": "baremetal", "target": "thumbv7m-none-eabi", "rmw": "zenoh" },
            "transport": [
                { "kind": "ethernet", "ip": "10.0.2.50/24", "rmw": "zenoh", "locator": "tcp/10.0.2.2:7447" },
                { "kind": "serial", "device": "UART0", "baudrate": 115200, "rmw": "cyclonedds" }
            ]
        });
        let build = schema_build_json(std::slice::from_ref(&overlay), None);
        assert_eq!(build["board"], "baremetal");
        assert_eq!(build["target"], "thumbv7m-none-eabi");
        let typed: PlanBuildOptions = serde_json::from_value(build).unwrap();
        assert!(typed.is_bridge());
        assert_eq!(typed.transports[0].ip.as_deref(), Some("10.0.2.50/24"));
        assert_eq!(typed.transports[1].baudrate, Some(115200));
        assert!(typed.validate_transports().is_empty());
    }

    #[test]
    fn schema_build_json_later_overlay_overrides_earlier() {
        let first = json!({ "build": { "board": "native" } });
        let second =
            json!({ "build": { "board": "freertos" }, "transport": [ { "kind": "ethernet" } ] });
        let build = schema_build_json(&[first, second], None);
        assert_eq!(build["board"], "freertos");
        assert_eq!(build["transports"].as_array().unwrap().len(), 1);
    }

    /// Phase 255 — `[system].rmw` (the SSoT) wins over the deprecated `[build].rmw`
    /// overlay; with no system.toml the overlay still drives (fallback).
    #[test]
    fn schema_build_json_system_toml_rmw_wins_over_build_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let st = dir.path().join("system.toml");
        std::fs::write(
            &st,
            "[system]\nname=\"d\"\nrmw=\"cyclonedds\"\ndomain_id=0\n",
        )
        .unwrap();
        let overlay = json!({ "build": { "rmw": "zenoh" } });

        let build = schema_build_json(std::slice::from_ref(&overlay), Some(&st));
        assert_eq!(
            build["rmw"], "cyclonedds",
            "[system].rmw must win over [build].rmw"
        );

        // No system.toml → the [build].rmw overlay drives (deprecated fallback).
        let fallback = schema_build_json(std::slice::from_ref(&overlay), None);
        assert_eq!(fallback["rmw"], "zenoh");
    }

    #[cfg(feature = "play-launch-parser")]
    #[test]
    fn plan_system_parses_launch_and_keeps_distinct_instances() {
        let root = temp_workspace("nros-plan-two-instances");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(
            &launch,
            r#"<launch>
  <node pkg="demo_pkg" exec="talker" name="talker_a" />
  <node pkg="demo_pkg" exec="talker" name="talker_b" />
</launch>"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "package": "demo_pkg",
  "component": "talker",
  "executable": "talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "publishers": [{
      "id": "pub.chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [],
    "timers": [],
    "services": [],
    "actions": []
  }]
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: None,
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();
        let instances = plan["instances"].as_array().unwrap();
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0]["id"], "demo_pkg.talker.0");
        assert_eq!(instances[1]["id"], "demo_pkg.talker.1");
    }

    /// Phase 254 — a `[safety]` capability declared in the bringup `system.toml`
    /// (the SSoT, not a per-package `nros.toml` overlay) lands in `plan.safety`.
    /// Uses a pre-built record (no launch parsing) so it runs in the default suite.
    #[test]
    fn plan_system_reads_safety_from_system_toml() {
        let root = temp_workspace("nros-plan-system-toml-safety");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        // The capability SSoT — typed system.toml, NOT nros.toml.
        fs::write(
            root.join("system.toml"),
            "[system]\nname=\"system_pkg\"\nrmw=\"zenoh\"\ndomain_id=0\n[safety]\ncrc=true\n",
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{ "node": [ { "package": "demo_pkg", "executable": "talker", "name": "worker", "namespace": "/" } ] }"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1, "package": "demo_pkg", "component": "talker", "language": "rust",
  "executable": "talker", "exported_symbol": "nros_component_talker",
  "nodes": [{
    "id": "node_talker", "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null, "publishers": [], "subscribers": [], "timers": [], "services": [], "actions": []
  }],
  "callbacks": [], "parameters": [],
  "trace": {"generator": "test", "package_manifest": "package.xml", "source_artifacts": []}
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();
        assert_eq!(
            plan["safety"]["crc"],
            serde_json::Value::Bool(true),
            "system.toml [safety] must land in plan.safety; got {plan:?}"
        );
    }

    #[cfg(feature = "play-launch-parser")]
    #[test]
    fn plan_system_resolves_private_remap_and_matches_manifest() {
        let root = temp_workspace("nros-plan-private-remap");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(
            &launch,
            r#"<launch>
  <node pkg="demo_pkg" exec="driver" name="driver" namespace="/robot">
    <remap from="~/cmd" to="/mux/cmd" />
  </node>
</launch>"#,
        )
        .unwrap();
        let metadata = root.join("driver.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "package": "demo_pkg",
  "component": "driver",
  "executable": "driver",
  "nodes": [{
    "id": "node_driver",
    "unresolved_name": {"value": "driver", "kind": "relative"},
    "publishers": [{
      "id": "pub.cmd",
      "unresolved_topic": {"value": "~/cmd", "kind": "private"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [],
    "timers": [{"id": "timer.poll", "period_ms": 100, "callback": "cb.poll"}],
    "services": [],
    "actions": []
  }],
  "callbacks": [{
    "id": "cb.poll",
    "kind": "timer",
    "group": null,
    "effects": [],
    "source": {"artifact": "src/driver.rs", "line": null, "column": null}
  }],
  "parameters": [],
  "trace": {"generator": "test", "package_manifest": "package.xml", "source_artifacts": ["src/driver.rs"]}
}"#,
        )
        .unwrap();
        let manifest = root.join("manifest.launch.yaml");
        fs::write(
            &manifest,
            r#"version: 1
topics:
  /mux/cmd:
    type: std_msgs/msg/String
    pub: [/robot/driver]
"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: None,
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![manifest],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();
        assert_eq!(
            plan["instances"][0]["nodes"][0]["entities"][0]["resolved_name"],
            "/mux/cmd"
        );
        assert_eq!(
            plan["instances"][0]["nodes"][0]["entities"][1]["role"],
            "timer"
        );
        assert!(
            plan["instances"][0]["nodes"][0]["entities"][1]
                .get("resolved_name")
                .is_none()
        );
    }

    #[cfg(feature = "play-launch-parser")]
    #[test]
    fn plan_system_rejects_metadata_entity_missing_from_manifest() {
        let root = temp_workspace("nros-plan-manifest-extra-entity");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(
            &launch,
            r#"<launch>
  <node pkg="demo_pkg" exec="talker" name="talker" />
</launch>"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "package": "demo_pkg",
  "component": "talker",
  "executable": "talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "publishers": [{
      "id": "pub_chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }, {
      "id": "pub_extra",
      "unresolved_topic": {"value": "extra", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [],
    "timers": [],
    "services": [],
    "actions": []
  }],
  "callbacks": [],
  "parameters": [],
  "trace": {"generator": "test", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
        )
        .unwrap();
        let manifest = root.join("manifest.launch.yaml");
        fs::write(
            &manifest,
            r#"version: 1
topics:
  /chatter:
    type: std_msgs/msg/String
    pub: [/talker]
"#,
        )
        .unwrap();

        let err = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: None,
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![manifest],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap_err()
        .to_string();

        assert!(err.contains("metadata-entity-unmatched"), "{err}");
        assert!(err.contains("/extra"), "{err}");
        assert!(err.contains("pub_extra"), "{err}");
    }

    #[cfg(feature = "play-launch-parser")]
    #[test]
    fn check_plan_rejects_missing_sched_context() {
        let (root, mut plan) = generated_plan("nros-check-missing-sched-context");
        plan["instances"][0]["callbacks"] = serde_json::json!([{
            "id": "demo_pkg.talker.0/cb",
            "source_callback": "cb",
            "group": "default",
            "sched_context": "missing_executor",
            "source": {
                "artifact": "talker.rs",
                "line": null,
                "column": null
            }
        }]);
        let plan_path = root.join("bad-plan.json");
        fs::write(&plan_path, serde_json::to_string_pretty(&plan).unwrap()).unwrap();

        let err = check_plan_file(&plan_path).unwrap_err().to_string();
        assert!(err.contains("missing-sched-context"), "{err}");
    }

    #[test]
    fn rmw_set_feasibility_warns_on_embedded_multi_rmw_only() {
        // Phase 172 WP-B slice 4 — `nros check` warns when >1 RMW links into one
        // embedded binary; hosted multi-RMW + single-RMW are silent.
        let root = temp_workspace("nros-rmw-set-feasibility");
        fs::create_dir_all(&root).unwrap();
        let plan = |board: &str, target: &str, rmws: &[&str]| -> Value {
            let transports: Vec<Value> = rmws
                .iter()
                .map(|r| json!({ "kind": "ethernet", "rmw": r }))
                .collect();
            json!({
                "version": 2, "system": "s",
                "trace": { "system_config": "nros.toml", "launch_record": "r", "generated_by": "t" },
                "components": [], "instances": [], "interfaces": [], "sched_contexts": [],
                "build": {
                    "target": target, "board": board, "rmw": "zenoh",
                    "profile": "release", "features": [], "cfg": {}, "transports": transports
                }
            })
        };
        let check = |value: Value, name: &str| -> CheckReport {
            let path = root.join(name);
            fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
            check_plan_file(&path).unwrap()
        };

        let embedded_multi = check(
            plan("freertos", "thumbv7m-none-eabi", &["zenoh", "cyclonedds"]),
            "embedded-multi.json",
        );
        assert_eq!(embedded_multi.warnings, 1, "{:?}", embedded_multi.messages);
        assert!(
            embedded_multi.messages[0].contains("RMW backends")
                && embedded_multi.messages[0].contains("cyclonedds"),
            "{:?}",
            embedded_multi.messages
        );

        let hosted_multi = check(
            plan(
                "native",
                "x86_64-unknown-linux-gnu",
                &["zenoh", "cyclonedds"],
            ),
            "hosted-multi.json",
        );
        assert_eq!(hosted_multi.warnings, 0, "{:?}", hosted_multi.messages);

        let embedded_single = check(
            plan("freertos", "thumbv7m-none-eabi", &["zenoh"]),
            "embedded-single.json",
        );
        assert_eq!(
            embedded_single.warnings, 0,
            "{:?}",
            embedded_single.messages
        );
    }

    #[cfg(feature = "play-launch-parser")]
    #[test]
    fn check_plan_rejects_unknown_interface_entity() {
        let (root, mut plan) = generated_plan("nros-check-missing-interface-entity");
        plan["interfaces"][0]["used_by"] = serde_json::json!(["missing/entity"]);
        let plan_path = root.join("bad-plan.json");
        fs::write(&plan_path, serde_json::to_string_pretty(&plan).unwrap()).unwrap();

        let err = check_plan_file(&plan_path).unwrap_err().to_string();
        assert!(err.contains("missing-interface-entity"), "{err}");
    }

    /// Verifies planning preserves instance callbacks, remaps, and parameter overrides.
    #[test]
    fn plan_system_keep_callback_remaps() {
        let root = temp_workspace("nros-plan-callbacks-params");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "node": [
    {
      "package": "demo_pkg",
      "executable": "talker",
      "name": "talker_a",
      "namespace": "/robot_a",
      "remaps": [{"from": "chatter", "to": "/bus/a"}],
      "params": [{"name": "rate_hz", "value": "20"}]
    },
    {
      "package": "demo_pkg",
      "executable": "talker",
      "name": "talker_b",
      "namespace": "/robot_b",
      "remaps": [{"from": "chatter", "to": "/bus/b"}],
      "params": [{"name": "rate_hz", "value": "30"}]
    }
  ]
}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": "nros_component_talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [{
      "id": "pub_chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [{
      "id": "sub_cmd",
      "unresolved_topic": {"value": "cmd", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null,
      "callback": "cb_cmd"
    }],
    "timers": [],
    "services": [],
    "actions": []
  }],
  "callbacks": [{
    "id": "cb_cmd",
    "kind": "subscription",
    "group": null,
    "effects": [],
    "source": {"artifact": "src/talker.rs", "line": 42, "column": 5}
  }],
  "parameters": [
    {"node": "node_talker", "name": "rate_hz", "default": 10, "read_only": false, "source": {"artifact": "src/talker.rs", "line": 10, "column": 1}},
    {"node": "node_talker", "name": "frame", "default": "map", "read_only": false, "source": {"artifact": "src/talker.rs", "line": 11, "column": 1}}
  ],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();
        let instances = plan["instances"].as_array().unwrap();
        assert_eq!(instances.len(), 2);
        assert_eq!(
            instances[0]["nodes"][0]["entities"][0]["resolved_name"],
            "/bus/a"
        );
        assert_eq!(
            instances[1]["nodes"][0]["entities"][0]["resolved_name"],
            "/bus/b"
        );
        assert_eq!(
            instances[0]["callbacks"][0]["id"],
            "demo_pkg.talker.0/cb_cmd"
        );
        assert_eq!(
            instances[0]["nodes"][0]["entities"][1]["callback"],
            "cb_cmd"
        );
        assert_eq!(
            instances[1]["callbacks"][0]["id"],
            "demo_pkg.talker.1/cb_cmd"
        );
        assert_eq!(
            instances[0]["sched_bindings"][0]["callback"],
            "demo_pkg.talker.0/cb_cmd"
        );
        assert_plan_parameter(&instances[0], "rate_hz", json!(20));
        assert_plan_parameter(&instances[1], "rate_hz", json!(30));
        assert_plan_parameter(&instances[0], "frame", json!("map"));
    }

    /// Phase 211.H — `qos_overrides.<topic>.<role>.<policy>` launch params are
    /// split out of the generic `parameters` table into the typed
    /// `qos_overrides` block, and the topic (which contains `/`, not `.`) is
    /// recovered intact from the dotted param name.
    #[test]
    fn qos_overrides_split_from_parameters_and_decompose() {
        let params = json!({
            "rate_hz": 10,
            "qos_overrides./chatter.publisher.reliability": "reliable",
            "qos_overrides./chatter.publisher.depth": 5,
            "qos_overrides./scan/points.subscription.durability": "transient_local",
            "parameter_files": "ignored.yaml"
        });

        // Generic params: only the non-qos, non-metadata one survives.
        let plain = schema_parameters("node_x", Some(&params));
        let names: Vec<&str> = plain
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["rate_hz"], "qos_overrides leaked into parameters");

        // QoS overrides: decomposed + topic with `/` preserved, sorted.
        let qos = schema_qos_overrides(Some(&params));
        assert_eq!(qos.len(), 3, "expected 3 qos overrides, got {qos:?}");

        // Sorted by (topic, role, policy): /chatter.publisher.depth first.
        assert_eq!(qos[0]["topic"], "/chatter");
        assert_eq!(qos[0]["role"], "publisher");
        assert_eq!(qos[0]["policy"], "depth");
        assert_eq!(qos[0]["value"], json!(5));

        assert_eq!(qos[1]["topic"], "/chatter");
        assert_eq!(qos[1]["policy"], "reliability");
        assert_eq!(qos[1]["value"], json!("reliable"));

        // Multi-segment topic `/scan/points` recovered intact.
        assert_eq!(qos[2]["topic"], "/scan/points");
        assert_eq!(qos[2]["role"], "subscription");
        assert_eq!(qos[2]["policy"], "durability");
        assert_eq!(qos[2]["value"], json!("transient_local"));

        // Malformed (no role/policy) → skipped, not panicked.
        let bad = json!({ "qos_overrides./only_topic": "x" });
        assert!(schema_qos_overrides(Some(&bad)).is_empty());
    }

    /// Phase 211.F — a launch `<node machine="…">` (recorded by
    /// play_launch_parser as `node.machine`) flows through `plan_system` into
    /// `instances[*].host_id`; a node without `machine` omits the field.
    #[test]
    fn plan_system_lowers_machine_to_host_id() {
        let root = temp_workspace("nros-plan-host-id");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "node": [
    { "package": "demo_pkg", "executable": "talker", "name": "talker_a",
      "namespace": "/robot_a", "remaps": [], "params": [], "machine": "robot1" },
    { "package": "demo_pkg", "executable": "talker", "name": "talker_b",
      "namespace": "/robot_b", "remaps": [], "params": [] }
  ]
}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1, "package": "demo_pkg", "component": "talker", "language": "rust",
  "executable": "talker", "exported_symbol": "nros_component_talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [{
      "id": "pub_chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [], "timers": [], "services": [], "actions": []
  }],
  "callbacks": [], "parameters": [],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();

        let instances = plan["instances"].as_array().unwrap();
        let with_host = instances
            .iter()
            .find(|i| i["launch_name"].as_str() == Some("/robot_a/talker_a"))
            .or_else(|| instances.iter().find(|i| i["host_id"].is_string()))
            .expect("instance with machine");
        assert_eq!(with_host["host_id"], json!("robot1"));

        // The machine-less node omits host_id entirely.
        let without = instances
            .iter()
            .find(|i| i["host_id"].is_null() || i.get("host_id").is_none())
            .expect("a host_id-less instance");
        assert!(without.get("host_id").is_none(), "host_id should be omitted when no machine");
    }

    /// Phase 211.H — end-to-end planner: a launch `<param qos_overrides…>` (as
    /// the parser records it) flows through `plan_system` into the typed
    /// `instances[*].qos_overrides` block, decomposed + split from `parameters`.
    #[test]
    fn plan_system_lowers_qos_overrides() {
        let root = temp_workspace("nros-plan-qos-overrides");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "node": [
    {
      "package": "demo_pkg",
      "executable": "talker",
      "name": "talker_a",
      "namespace": "/robot_a",
      "remaps": [],
      "params": [
        {"name": "rate_hz", "value": "20"},
        {"name": "qos_overrides./chatter.publisher.reliability", "value": "best_effort"},
        {"name": "qos_overrides./chatter.publisher.depth", "value": "5"}
      ]
    }
  ]
}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1, "package": "demo_pkg", "component": "talker", "language": "rust",
  "executable": "talker", "exported_symbol": "nros_component_talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [{
      "id": "pub_chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [], "timers": [], "services": [], "actions": []
  }],
  "callbacks": [],
  "parameters": [],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        // Round-trips through the typed schema (validates the additive field).
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();

        let inst = &plan["instances"][0];
        // The qos params are SPLIT OUT of `parameters` (only rate_hz remains).
        let pnames: Vec<&str> = inst["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(pnames, vec!["rate_hz"], "qos param leaked into parameters");

        // ... and lowered into the typed qos_overrides block, decomposed.
        let qos = inst["qos_overrides"].as_array().expect("qos_overrides block");
        assert_eq!(qos.len(), 2, "got {qos:?}");
        // Sorted (topic, role, policy): depth before reliability.
        assert_eq!(qos[0]["topic"], "/chatter");
        assert_eq!(qos[0]["role"], "publisher");
        assert_eq!(qos[0]["policy"], "depth");
        assert_eq!(qos[1]["policy"], "reliability");
        assert_eq!(qos[1]["value"], json!("best_effort"));
    }

    #[test]
    fn plan_system_generates_ids_from_declaration_slots() {
        let root = temp_workspace("nros-plan-generated-slot-ids");
        let output = plan_with_record_and_metadata(
            &root,
            r#"{
  "node": [{
    "package": "demo_pkg",
    "executable": "talker",
    "name": "talker",
    "params": [{"name": "rate_hz", "value": "20"}]
  }]
}"#,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": null,
  "nodes": [{
    "id": "node_talker",
    "declaration_slot": 3,
    "source_default_name": "talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [{
      "id": "pub_chatter",
      "declaration_slot": 4,
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [{
      "id": "sub_cmd",
      "declaration_slot": 5,
      "unresolved_topic": {"value": "cmd", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null,
      "callback": "cb_cmd",
      "callback_slot": 8
    }],
    "timers": [{
      "id": "timer_poll",
      "declaration_slot": 6,
      "period_ms": 100,
      "callback": "cb_tick",
      "callback_slot": 9
    }],
    "services": [],
    "actions": []
  }],
  "callbacks": [{
    "id": "cb_cmd",
    "declaration_slot": 8,
    "kind": "subscription",
    "group": null,
    "effects": [],
    "source": {"artifact": "src/talker.rs", "line": 42, "column": 5}
  }, {
    "id": "cb_tick",
    "declaration_slot": 9,
    "kind": "timer",
    "group": null,
    "effects": [],
    "source": {"artifact": "src/talker.rs", "line": 50, "column": 5}
  }],
  "parameters": [
    {"node": "node_talker", "name": "rate_hz", "default": 10, "read_only": false, "source": {"artifact": "src/talker.rs", "line": 10, "column": 1}}
  ],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
        )
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();

        let instance = &plan["instances"][0];
        let node = &instance["nodes"][0];
        assert_eq!(node["id"], "demo_pkg.talker.0/node_3");
        assert_eq!(node["source_node"], "node_talker");
        assert_eq!(node["declaration_slot"], 3);
        assert_eq!(node["source_default_name"], "talker");

        let entities = node["entities"].as_array().unwrap();
        assert_eq!(entities[0]["id"], "demo_pkg.talker.0/entity_4");
        assert_eq!(entities[0]["source_entity"], "pub_chatter");
        assert_eq!(entities[1]["id"], "demo_pkg.talker.0/entity_5");
        assert_eq!(entities[1]["source_entity"], "sub_cmd");
        assert_eq!(entities[1]["callback"], "demo_pkg.talker.0/callback_8");
        assert_eq!(entities[2]["id"], "demo_pkg.talker.0/entity_6");
        assert_eq!(entities[2]["source_entity"], "timer_poll");
        assert_eq!(entities[2]["callback"], "demo_pkg.talker.0/callback_9");

        let callbacks = instance["callbacks"].as_array().unwrap();
        assert_eq!(callbacks[0]["id"], "demo_pkg.talker.0/callback_8");
        assert_eq!(callbacks[0]["source_callback"], "cb_cmd");
        assert_eq!(callbacks[1]["id"], "demo_pkg.talker.0/callback_9");
        assert_eq!(callbacks[1]["source_callback"], "cb_tick");
        assert_eq!(
            instance["sched_bindings"][0]["callback"],
            "demo_pkg.talker.0/callback_8"
        );
        assert_eq!(
            instance["sched_bindings"][1]["callback"],
            "demo_pkg.talker.0/callback_9"
        );

        let parameter = instance["parameters"].as_array().unwrap().first().unwrap();
        assert_eq!(parameter["node"], "demo_pkg.talker.0/node_3");
        assert_eq!(parameter["name"], "rate_hz");
    }

    fn assert_plan_parameter(instance: &Value, name: &str, expected: Value) {
        let parameter = instance["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|parameter| parameter["name"] == name)
            .unwrap_or_else(|| panic!("missing parameter {name}"));
        assert_eq!(parameter["value"], expected);
    }

    /// Phase 211.E — `<set_env>` / `<env>` declarations in the launch file
    /// land on each instance's `env` array as `{name, value}` objects.
    /// Without the propagation the deploy stage has no way to ship the
    /// declared env onto the spawned process.
    #[test]
    fn plan_system_threads_node_env_onto_instances() {
        let root = temp_workspace("nros-plan-set-env");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        // Record shape mirrors the parser output for
        //     <set_env name="DEMO_LEVEL" value="verbose" />
        //     <node pkg="demo_pkg" exec="talker" name="worker">
        //       <env name="NODE_VAR" value="node_specific" />
        //     </node>
        // i.e. one merged `env = [[k, v], …]` per record.node entry.
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "node": [
    {
      "package": "demo_pkg",
      "executable": "talker",
      "name": "worker",
      "namespace": "/",
      "env": [
        ["DEMO_LEVEL", "verbose"],
        ["NODE_VAR", "node_specific"]
      ]
    }
  ]
}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": "nros_component_talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [],
    "subscribers": [],
    "timers": [],
    "services": [],
    "actions": []
  }],
  "callbacks": [],
  "parameters": [],
  "trace": {"generator": "test", "package_manifest": "package.xml", "source_artifacts": []}
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        let instances = plan["instances"].as_array().unwrap();
        assert_eq!(instances.len(), 1);
        let env = instances[0]["env"]
            .as_array()
            .expect("env field must be an array on the instance");
        // Both pairs must propagate, in order, as {name, value} objects.
        assert_eq!(env.len(), 2);
        assert_eq!(env[0]["name"], "DEMO_LEVEL");
        assert_eq!(env[0]["value"], "verbose");
        assert_eq!(env[1]["name"], "NODE_VAR");
        assert_eq!(env[1]["value"], "node_specific");
    }

    /// Phase 211.B — `<node_container>` mints a container instance; its
    /// `<composable_node>` children land as flat instances but each
    /// carries `container_id` pointing back at the parent and
    /// `kind = "composable_node"`. The container itself has
    /// `kind = "container"` and NO `container_id`.
    #[test]
    fn plan_system_groups_composables_under_container() {
        let root = temp_workspace("nros-plan-composable-grouping");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        // Mirrors the parser output for:
        //   <node_container pkg="rclcpp_components" exec="component_container"
        //                    name="my_container" namespace="">
        //     <composable_node pkg="demo_pkg" plugin="demo_pkg::Talker" name="talker"/>
        //     <composable_node pkg="demo_pkg" plugin="demo_pkg::Listener" name="listener"/>
        //   </node_container>
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "container": [
    {
      "package": "rclcpp_components",
      "executable": "component_container",
      "name": "my_container",
      "namespace": "/"
    }
  ],
  "load_node": [
    {
      "package": "demo_pkg",
      "plugin": "demo_pkg::Talker",
      "node_name": "talker",
      "namespace": "/",
      "target_container_name": "/my_container"
    },
    {
      "package": "demo_pkg",
      "plugin": "demo_pkg::Listener",
      "node_name": "listener",
      "namespace": "/",
      "target_container_name": "/my_container"
    }
  ]
}"#,
        )
        .unwrap();
        let make_metadata = |path: &Path, component: &str| {
            fs::write(
                path,
                format!(
                    r#"{{
  "version": 1, "package": "demo_pkg", "component": "{component}", "language": "cpp",
  "executable": "{component}", "exported_symbol": "nros_component_demo_pkg_{component}",
  "nodes": [{{ "id": "n", "unresolved_name": {{"value":"{component}","kind":"relative"}}, "namespace": null,
    "publishers": [], "subscribers": [], "timers": [], "services": [], "actions": [] }}],
  "callbacks": [], "parameters": [],
  "trace": {{"generator":"test","package_manifest":"package.xml","source_artifacts":[]}}
}}"#
                ),
            )
            .unwrap();
        };
        let container_md = root.join("container.metadata.json");
        fs::write(
            &container_md,
            r#"{
  "version": 1, "package": "rclcpp_components", "component": "component_container", "language": "cpp",
  "executable": "component_container", "exported_symbol": "nros_component_container",
  "nodes": [{ "id": "n", "unresolved_name": {"value":"component_container","kind":"relative"}, "namespace": null,
    "publishers": [], "subscribers": [], "timers": [], "services": [], "actions": [] }],
  "callbacks": [], "parameters": [],
  "trace": {"generator":"test","package_manifest":"package.xml","source_artifacts":[]}
}"#,
        )
        .unwrap();
        let talker_md = root.join("talker.metadata.json");
        make_metadata(&talker_md, "Talker");
        let listener_md = root.join("listener.metadata.json");
        make_metadata(&listener_md, "Listener");

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![container_md, talker_md, listener_md],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        // Schema round-trip catches drift (deny_unknown_fields).
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();

        let instances = plan["instances"].as_array().unwrap();
        assert_eq!(
            instances.len(),
            3,
            "expected container + 2 composables, got: {instances:#?}"
        );

        let container = instances
            .iter()
            .find(|i| i["kind"] == "container")
            .expect("container instance");
        assert_eq!(
            container["component"],
            "rclcpp_components::component_container"
        );
        assert!(
            container.get("container_id").is_none() || container["container_id"].is_null(),
            "container must NOT carry its own container_id: {container:#?}"
        );
        let container_id = container["id"].as_str().expect("container id");

        for needle in ["Talker", "Listener"] {
            let child = instances
                .iter()
                .find(|i| {
                    i["component"]
                        .as_str()
                        .is_some_and(|s| s == format!("demo_pkg::{needle}"))
                })
                .unwrap_or_else(|| panic!("no demo_pkg::{needle} instance"));
            assert_eq!(
                child["kind"], "composable_node",
                "{needle} should be kind=composable_node"
            );
            assert_eq!(
                child["container_id"], container_id,
                "{needle} container_id must point at the parent container"
            );
        }
    }

    /// A plain `<node>` (no parent container) must surface as
    /// `kind = "node"` with no `container_id` key on the JSON (the field
    /// is `skip_serializing_if = "Option::is_none"` so byte-compat with
    /// pre-211.B plans is preserved).
    #[test]
    fn plan_system_plain_node_has_kind_node_and_no_container_id() {
        let root = temp_workspace("nros-plan-plain-node-kind");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "node": [{
    "package": "demo_pkg",
    "executable": "talker",
    "name": "talker",
    "namespace": "/"
  }]
}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1, "package": "demo_pkg", "component": "talker", "language": "rust",
  "executable": "talker", "exported_symbol": "nros_component_talker",
  "nodes": [{ "id": "n", "unresolved_name": {"value":"talker","kind":"relative"}, "namespace": null,
    "publishers": [], "subscribers": [], "timers": [], "services": [], "actions": [] }],
  "callbacks": [], "parameters": [],
  "trace": {"generator":"test","package_manifest":"package.xml","source_artifacts":[]}
}"#,
        )
        .unwrap();
        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let raw = fs::read_to_string(output.plan_path).unwrap();
        let plan: Value = serde_json::from_str(&raw).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();
        assert_eq!(plan["instances"][0]["kind"], "node");
        assert!(
            plan["instances"][0].get("container_id").is_none(),
            "container_id key must be omitted for plain <node>; got raw: {raw}"
        );
    }

    /// Phase 211.E — `<executable>` declarations surface on `plan.executables`
    /// as non-rmw spawn entries. Previously the parser-recorded
    /// `package=None` tripped a `missing-package` diagnostic, making any
    /// launch carrying an `<executable>` unplanable.
    #[test]
    fn plan_system_emits_executables_for_package_less_record_nodes() {
        let root = temp_workspace("nros-plan-executables");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        // Mirrors the parser output for:
        //   <set_env name="FOO" value="bar" />
        //   <executable cmd="/bin/echo" name="greeter">
        //     <arg value="hello" />
        //     <arg value="world" />
        //   </executable>
        fs::write(
            &record,
            r#"{
  "node": [
    {
      "package": null,
      "name": "greeter",
      "exec_name": "greeter",
      "executable": "/bin/echo",
      "cmd": ["/bin/echo", "hello", "world"],
      "args": ["hello", "world"],
      "env": [["FOO", "bar"]],
      "namespace": null
    }
  ]
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();

        // No rmw instances at all (the only record.node was the executable).
        assert_eq!(plan["instances"].as_array().unwrap().len(), 0);

        let execs = plan["executables"]
            .as_array()
            .expect("executables field must surface when the record carries any <executable>");
        assert_eq!(execs.len(), 1);
        let exec = &execs[0];
        assert_eq!(exec["id"], "executable.greeter.0");
        assert_eq!(exec["name"], "greeter");
        assert_eq!(exec["namespace"], "/");
        assert_eq!(exec["cmd"], json!(["/bin/echo", "hello", "world"]));
        assert_eq!(exec["args"], json!(["hello", "world"]));
        assert_eq!(exec["env"], json!([{"name": "FOO", "value": "bar"}]));
        assert_eq!(
            exec["trace"]["launch_record_entity"],
            "record://executable.greeter.0"
        );
    }

    /// A plan with no `<executable>` entries must NOT carry the `executables`
    /// key at all (additive field, `skip_serializing_if = "Vec::is_empty"`),
    /// so plans written before 211.E stay byte-identical.
    #[test]
    fn plan_system_omits_executables_field_when_none_declared() {
        let root = temp_workspace("nros-plan-executables-empty");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(
            &launch,
            r#"<launch>
  <node pkg="demo_pkg" exec="talker" name="talker" />
</launch>"#,
        )
        .unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "node": [{
    "package": "demo_pkg",
    "executable": "talker",
    "name": "talker",
    "namespace": "/"
  }]
}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1, "package": "demo_pkg", "component": "talker", "language": "rust",
  "executable": "talker", "exported_symbol": "nros_component_talker",
  "nodes": [{ "id": "n", "unresolved_name": {"value":"talker","kind":"relative"}, "namespace": null,
    "publishers": [], "subscribers": [], "timers": [], "services": [], "actions": [] }],
  "callbacks": [], "parameters": [],
  "trace": {"generator":"test","package_manifest":"package.xml","source_artifacts":[]}
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let raw = fs::read_to_string(output.plan_path).unwrap();
        let plan: Value = serde_json::from_str(&raw).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();
        assert!(
            plan.get("executables").is_none(),
            "expected `executables` to be omitted when none declared, got: {raw}"
        );
    }

    /// A record node without an `env` block must still emit an `env` field
    /// on the instance — empty, not null — so the deploy stage can iterate
    /// uniformly without a presence check.
    #[test]
    fn plan_system_emits_empty_env_when_record_has_none() {
        let root = temp_workspace("nros-plan-set-env-empty");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{
  "node": [
    {
      "package": "demo_pkg",
      "executable": "talker",
      "name": "worker",
      "namespace": "/"
    }
  ]
}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": "nros_component_talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [],
    "subscribers": [],
    "timers": [],
    "services": [],
    "actions": []
  }],
  "callbacks": [],
  "parameters": [],
  "trace": {"generator": "test", "package_manifest": "package.xml", "source_artifacts": []}
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        let env = plan["instances"][0]["env"].as_array().expect("env array");
        assert!(env.is_empty());
    }

    #[test]
    fn plan_system_rejects_unknown_callback_effect_entity() {
        let root = temp_workspace("nros-plan-bad-callback-effect");
        let err = plan_with_metadata(
            &root,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": null,
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [{
      "id": "pub_chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [],
    "timers": [],
    "services": [],
    "actions": []
  }],
  "callbacks": [{
    "id": "cb_timer",
    "kind": "timer",
    "group": null,
    "effects": [{"kind": "publishes", "entity": "missing_pub"}],
    "source": {"artifact": "src/talker.rs", "line": 42, "column": 5}
  }],
  "parameters": [],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("callback-effect-unknown-entity"), "{err}");
        assert!(err.contains("missing_pub"), "{err}");
    }

    #[test]
    fn plan_system_rejects_missing_entity_callback() {
        let root = temp_workspace("nros-plan-missing-entity-callback");
        let err = plan_with_metadata(
            &root,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": null,
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [],
    "subscribers": [{
      "id": "sub_cmd",
      "unresolved_topic": {"value": "cmd", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null,
      "callback": "cb_missing"
    }],
    "timers": [],
    "services": [],
    "actions": []
  }],
  "callbacks": [],
  "parameters": [],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("entity-callback-missing"), "{err}");
        assert!(err.contains("cb_missing"), "{err}");
    }

    #[test]
    fn plan_system_preserves_multiple_source_nodes() {
        let root = temp_workspace("nros-plan-multiple-source-nodes");
        let output = plan_with_metadata(
            &root,
            r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": null,
  "nodes": [
    {
      "id": "node_talker",
      "unresolved_name": {"value": "talker", "kind": "relative"},
      "namespace": null,
      "publishers": [{
        "id": "pub_chatter",
        "unresolved_topic": {"value": "chatter", "kind": "relative"},
        "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
        "qos": null
      }],
      "subscribers": [],
      "timers": [],
      "services": [],
      "actions": []
    },
    {
      "id": "node_aux",
      "unresolved_name": {"value": "aux", "kind": "relative"},
      "namespace": null,
      "publishers": [],
      "subscribers": [],
      "timers": [],
      "services": [{
        "id": "srv_reset",
        "unresolved_name": {"value": "reset", "kind": "relative"},
        "interface": {"package": "std_srvs", "name": "srv/Trigger", "kind": "service"},
        "callback": "cb_reset"
      }],
      "actions": [{
        "id": "act_nav",
        "unresolved_name": {"value": "navigate", "kind": "relative"},
        "interface": {"package": "nav2_msgs", "name": "action/NavigateToPose", "kind": "action"},
        "goal_callback": "cb_nav_goal",
        "cancel_callback": "cb_nav_cancel",
        "accepted_callback": "cb_nav_accepted"
      }]
    }
  ],
  "callbacks": [
    {"id": "cb_reset", "kind": "service", "group": null, "effects": [{"kind": "sends_service_reply", "entity": "srv_reset"}], "source": {"artifact": "src/lib.rs", "line": 10, "column": 1}},
    {"id": "cb_nav_goal", "kind": "action_goal", "group": null, "effects": [{"kind": "sends_action_goal", "entity": "act_nav"}], "source": {"artifact": "src/lib.rs", "line": 20, "column": 1}},
    {"id": "cb_nav_cancel", "kind": "action_cancel", "group": null, "effects": [], "source": {"artifact": "src/lib.rs", "line": 30, "column": 1}},
    {"id": "cb_nav_accepted", "kind": "action_accepted", "group": null, "effects": [{"kind": "sends_action_result", "entity": "act_nav"}], "source": {"artifact": "src/lib.rs", "line": 40, "column": 1}}
  ],
  "parameters": [],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/lib.rs"]}
}"#,
        )
        .unwrap();
        let plan: Value =
            serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        serde_json::from_value::<NrosPlan>(plan.clone()).unwrap();

        let nodes = plan["instances"][0]["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["source_node"], "node_talker");
        assert_eq!(nodes[0]["resolved_name"], "/talker");
        assert_eq!(
            nodes[0]["entities"][0]["id"],
            "demo_pkg.talker.0/pub_chatter"
        );
        assert_eq!(nodes[1]["source_node"], "node_aux");
        assert_eq!(nodes[1]["resolved_name"], "/aux");
        assert_eq!(nodes[1]["entities"][0]["role"], "service_server");
        assert_eq!(nodes[1]["entities"][1]["role"], "action_server");
    }

    fn plan_with_metadata(root: &Path, metadata_json: &str) -> Result<PlanningOutput> {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(
            &record,
            r#"{"node":[{"package":"demo_pkg","executable":"talker","name":"talker"}]}"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(&metadata, metadata_json).unwrap();

        plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.to_path_buf(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
    }

    fn plan_with_record_and_metadata(
        root: &Path,
        record_json: &str,
        metadata_json: &str,
    ) -> Result<PlanningOutput> {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(&launch, "<launch />").unwrap();
        let record = root.join("record.json");
        fs::write(&record, record_json).unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(&metadata, metadata_json).unwrap();

        plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.to_path_buf(),
            launch_file: launch,
            record_file: Some(record),
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
    }

    fn basic_talker_metadata(source_node_name: &str) -> String {
        format!(
            r#"{{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": null,
  "nodes": [{{
    "id": "node_talker",
    "unresolved_name": {{"value": "{source_node_name}", "kind": "relative"}},
    "namespace": null,
    "publishers": [],
    "subscribers": [],
    "timers": [],
    "services": [],
    "actions": []
  }}],
  "callbacks": [],
  "parameters": [],
  "trace": {{"generator": "test", "package_manifest": "package.xml", "source_artifacts": []}}
}}"#
        )
    }

    #[cfg(feature = "play-launch-parser")]
    fn generated_plan(name: &str) -> (PathBuf, Value) {
        let root = temp_workspace(name);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.xml"),
            r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        let launch = root.join("system.launch.xml");
        fs::write(
            &launch,
            r#"<launch>
  <node pkg="demo_pkg" exec="talker" name="talker" />
</launch>"#,
        )
        .unwrap();
        let metadata = root.join("talker.metadata.json");
        fs::write(
            &metadata,
            r#"{
  "package": "demo_pkg",
  "component": "talker",
  "executable": "talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "publishers": [{
      "id": "pub.chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [],
    "timers": [],
    "services": [],
    "actions": []
  }]
}"#,
        )
        .unwrap();

        let output = plan_system(PlanOptions {
            system_pkg: "system_pkg".to_string(),
            workspace_root: root.clone(),
            launch_file: launch,
            record_file: None,
            out_root: root.join("build/system_pkg/nros"),
            metadata_files: vec![metadata],
            manifest_files: vec![],
            nros_toml_files: vec![],
            launch_args: vec![],
        })
        .unwrap();
        let plan = serde_json::from_str(&fs::read_to_string(output.plan_path).unwrap()).unwrap();
        (root, plan)
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{}-{stamp}", std::process::id()))
    }

    // ---- Phase 212.M-F.20 — synthesized ament prefix for find-pkg-share ----

    /// `synthesize_workspace_ament_prefix` must lay down a valid ament prefix
    /// from the workspace pkg-index: an empty resource-index marker per package
    /// plus a `share/<pkg>` symlink to the package source dir, so the launch
    /// parser's `$(find-pkg-share <pkg>)` resolves an in-tree, never-installed
    /// package (`<prefix>/share/<pkg>/launch/...`).
    #[test]
    fn ament_prefix_synthesis_maps_pkg_share_to_source() {
        let ws = temp_workspace("mf20_ament_prefix_ws");
        let pkg_dir = ws.join("src/secondary_node");
        fs::create_dir_all(pkg_dir.join("launch")).unwrap();
        // Cargo workspace marker so `build_pkg_index` accepts the root.
        fs::write(ws.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        fs::write(
            pkg_dir.join("package.xml"),
            "<?xml version=\"1.0\"?><package format=\"3\">\
             <name>secondary_node</name></package>",
        )
        .unwrap();
        fs::write(pkg_dir.join("launch/secondary.launch.xml"), "<launch/>").unwrap();

        let prefix = synthesize_workspace_ament_prefix(&ws)
            .expect("synthesis ok")
            .expect("a non-empty workspace yields a prefix");
        let root = prefix.path();

        // ament resource-index marker present for the discovered package.
        assert!(
            root.join("share/ament_index/resource_index/packages/secondary_node")
                .is_file(),
            "missing ament resource-index marker"
        );
        // `share/<pkg>` resolves to the source dir, so the launch file under it
        // is reachable through the prefix — exactly what `find-pkg-share` reads.
        let resolved = root.join("share/secondary_node/launch/secondary.launch.xml");
        assert!(
            resolved.exists(),
            "share/<pkg> symlink does not expose the package's launch file: {}",
            resolved.display()
        );

        let _ = fs::remove_dir_all(&ws);
    }

    /// A directory with no `package.xml` yields no prefix (the parser then keeps
    /// the ambient `AMENT_PREFIX_PATH` untouched).
    #[test]
    fn ament_prefix_synthesis_is_none_for_empty_workspace() {
        let ws = temp_workspace("mf20_empty_ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        let prefix = synthesize_workspace_ament_prefix(&ws).expect("synthesis ok");
        assert!(prefix.is_none(), "no packages → no synthesized prefix");
        let _ = fs::remove_dir_all(&ws);
    }

    // ---- Phase 172.B — callback-chain inference ----

    /// Verifies callback-chain inference links publisher instances to subscriber callbacks.
    #[test]
    fn infer_publisher_to_subscriber_chains() {
        // Instance `a`: a timer callback `a/tick` and a publisher on /chatter.
        // Instance `b`: a subscriber on /chatter bound to `b/on_msg`.
        // The timer (a producing callback of the publishing instance) should
        // chain into the downstream subscriber callback.
        let instances = vec![
            json!({
                "id": "a",
                "nodes": [{ "entities": [
                    { "role": "timer", "id": "a/timer", "callback": "a/tick" },
                    { "role": "publisher", "id": "a/pub", "resolved_name": "/chatter" },
                ]}]
            }),
            json!({
                "id": "b",
                "nodes": [{ "entities": [
                    { "role": "subscriber", "id": "b/sub", "resolved_name": "/chatter", "callback": "b/on_msg" },
                ]}]
            }),
        ];
        let chains = infer_callback_chains(&instances);
        assert_eq!(chains.len(), 1, "expected one chain, got {chains:?}");
        let chain = &chains[0];
        assert_eq!(chain["id"], json!("chain/a/tick"));
        assert_eq!(chain["callbacks"], json!(["a/tick", "b/on_msg"]));
        assert_eq!(chain["inferred"], json!(true));
        assert_eq!(
            chain["links"],
            json!([{ "from": "a/tick", "to": "b/on_msg", "topic": "/chatter" }])
        );
    }

    #[test]
    fn infer_callback_chains_chains_three_stages() {
        // sensor(timer)->/raw->filter(sub)->/filtered->control(sub): one chain.
        let instances = vec![
            json!({ "id": "sensor", "nodes": [{ "entities": [
                { "role": "timer", "id": "s/t", "callback": "sensor/sample" },
                { "role": "publisher", "id": "s/p", "resolved_name": "/raw" },
            ]}]}),
            json!({ "id": "filter", "nodes": [{ "entities": [
                { "role": "subscriber", "id": "f/s", "resolved_name": "/raw", "callback": "filter/on_raw" },
                { "role": "publisher", "id": "f/p", "resolved_name": "/filtered" },
            ]}]}),
            json!({ "id": "control", "nodes": [{ "entities": [
                { "role": "subscriber", "id": "c/s", "resolved_name": "/filtered", "callback": "control/on_filtered" },
            ]}]}),
        ];
        let chains = infer_callback_chains(&instances);
        assert_eq!(
            chains.len(),
            1,
            "expected one connected chain, got {chains:?}"
        );
        assert_eq!(
            chains[0]["callbacks"],
            json!(["sensor/sample", "filter/on_raw", "control/on_filtered"])
        );
    }

    #[test]
    fn infer_callback_chains_empty_without_matching_pub_sub() {
        // Publishes /chatter but nobody subscribes → no chain.
        let instances = vec![json!({
            "id": "a",
            "nodes": [{ "entities": [
                { "role": "timer", "id": "a/timer", "callback": "a/tick" },
                { "role": "publisher", "id": "a/pub", "resolved_name": "/chatter" },
            ]}]
        })];
        assert!(infer_callback_chains(&instances).is_empty());
    }

    #[test]
    fn infer_callback_groups_chain_is_mutually_exclusive() {
        // a/tick -> /chatter -> b/on_msg: a dataflow-coupled chain becomes one
        // mutually-exclusive group spanning both callbacks.
        let instances = vec![
            json!({
                "id": "a",
                "callbacks": [{ "id": "a/tick" }],
                "nodes": [{ "entities": [
                    { "role": "timer", "id": "a/timer", "callback": "a/tick" },
                    { "role": "publisher", "id": "a/pub", "resolved_name": "/chatter" },
                ]}]
            }),
            json!({
                "id": "b",
                "callbacks": [{ "id": "b/on_msg" }],
                "nodes": [{ "entities": [
                    { "role": "subscriber", "id": "b/sub", "resolved_name": "/chatter", "callback": "b/on_msg" },
                ]}]
            }),
        ];
        let chains = infer_callback_chains(&instances);
        let groups = infer_callback_groups(&instances, &chains);
        assert_eq!(groups.len(), 1, "expected one chain group, got {groups:?}");
        assert_eq!(groups[0]["id"], json!("group/a/tick"));
        assert_eq!(groups[0]["kind"], json!("mutually_exclusive"));
        assert_eq!(groups[0]["callbacks"], json!(["a/tick", "b/on_msg"]));
        assert_eq!(groups[0]["inferred"], json!(true));
    }

    #[test]
    fn infer_callback_groups_chainless_callback_is_reentrant() {
        // A timer callback whose publish has no in-system subscriber → no chain
        // → its own reentrant singleton group.
        let instances = vec![json!({
            "id": "a",
            "callbacks": [{ "id": "a/tick" }],
            "nodes": [{ "entities": [
                { "role": "timer", "id": "a/timer", "callback": "a/tick" },
                { "role": "publisher", "id": "a/pub", "resolved_name": "/chatter" },
            ]}]
        })];
        let chains = infer_callback_chains(&instances);
        assert!(chains.is_empty());
        let groups = infer_callback_groups(&instances, &chains);
        assert_eq!(groups.len(), 1, "got {groups:?}");
        assert_eq!(groups[0]["id"], json!("group/a/tick"));
        assert_eq!(groups[0]["kind"], json!("reentrant"));
        assert_eq!(groups[0]["callbacks"], json!(["a/tick"]));
        assert_eq!(groups[0]["inferred"], json!(true));
    }

    #[test]
    fn infer_callback_groups_mixes_chain_and_reentrant() {
        // a/tick -> b/on_msg chain plus an independent c/work timer.
        let instances = vec![
            json!({
                "id": "a",
                "callbacks": [{ "id": "a/tick" }],
                "nodes": [{ "entities": [
                    { "role": "timer", "id": "a/timer", "callback": "a/tick" },
                    { "role": "publisher", "id": "a/pub", "resolved_name": "/chatter" },
                ]}]
            }),
            json!({
                "id": "b",
                "callbacks": [{ "id": "b/on_msg" }],
                "nodes": [{ "entities": [
                    { "role": "subscriber", "id": "b/sub", "resolved_name": "/chatter", "callback": "b/on_msg" },
                ]}]
            }),
            json!({
                "id": "c",
                "callbacks": [{ "id": "c/work" }],
                "nodes": [{ "entities": [
                    { "role": "timer", "id": "c/timer", "callback": "c/work" },
                ]}]
            }),
        ];
        let chains = infer_callback_chains(&instances);
        let groups = infer_callback_groups(&instances, &chains);
        assert_eq!(
            groups.len(),
            2,
            "expected chain + reentrant group, got {groups:?}"
        );
        let me = groups
            .iter()
            .find(|g| g["kind"] == json!("mutually_exclusive"))
            .expect("a mutually-exclusive chain group");
        assert_eq!(me["callbacks"], json!(["a/tick", "b/on_msg"]));
        let re = groups
            .iter()
            .find(|g| g["kind"] == json!("reentrant"))
            .expect("a reentrant singleton group");
        assert_eq!(re["id"], json!("group/c/work"));
        assert_eq!(re["callbacks"], json!(["c/work"]));
    }

    #[test]
    fn collect_sched_contexts_reads_dedups_and_normalizes_tiers() {
        let overlays = vec![
            json!({ "scheduling": { "contexts": [
                { "id": "io", "class": "real_time", "priority": 10, "period_ms": 20 },
            ]}}),
            json!({ "scheduling": { "contexts": [
                { "id": "io", "class": "real_time", "priority": 99 }, // last-wins override
                { "id": "bg", "class": "best_effort" },
            ]}}),
        ];
        let (contexts, by_id) = collect_sched_contexts(&overlays);
        // Declaration order preserved: io (first declared), then bg.
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0]["id"], json!("io"));
        assert_eq!(contexts[1]["id"], json!("bg"));
        // Later overlay overrides the earlier `io`.
        assert_eq!(by_id["io"]["priority"], json!(99));
        // Absent keys normalised to defaults / null so the value round-trips.
        assert_eq!(contexts[1]["executor"], json!("single_threaded"));
        assert_eq!(contexts[1]["deadline_policy"], json!("ignore"));
        assert_eq!(contexts[1]["priority"], json!(null));
        assert_eq!(contexts[1]["period_ms"], json!(null));
    }

    #[test]
    fn schema_callbacks_binds_group_to_declared_tier() {
        let declared: std::collections::BTreeMap<String, Value> = [(
            "io".to_string(),
            normalize_sched_context(&json!({ "id": "io", "priority": 10 })),
        )]
        .into_iter()
        .collect();
        let callbacks = json!([
            { "id": "cb_io",   "group": "io" },
            { "id": "cb_main", "group": "main" },
        ]);
        let out = schema_callbacks("inst", Some(&callbacks), &declared);
        // group "io" matches a declared tier → bound there.
        assert_eq!(out[0]["sched_context"], json!("io"));
        // group "main" has no tier → falls back to default_executor.
        assert_eq!(out[1]["sched_context"], json!("default_executor"));
    }

    #[test]
    fn schema_sched_bindings_tags_declared_tier_vs_fallback() {
        let declared: std::collections::BTreeMap<String, Value> = [(
            "io".to_string(),
            normalize_sched_context(&json!({ "id": "io", "priority": 10 })),
        )]
        .into_iter()
        .collect();
        let callbacks = vec![
            json!({ "id": "inst/cb_io",   "sched_context": "io" }),
            json!({ "id": "inst/cb_main", "sched_context": "default_executor" }),
        ];
        let bindings = schema_sched_bindings(&callbacks, &declared);
        // Bound to a declared tier: carries its priority + nros.toml source.
        assert_eq!(bindings[0]["context"], json!("io"));
        assert_eq!(bindings[0]["priority"], json!(10));
        assert_eq!(bindings[0]["source"], json!("nros.toml"));
        // Fallback: pre-172.G null priority + source_metadata (byte-compat).
        assert_eq!(bindings[1]["context"], json!("default_executor"));
        assert_eq!(bindings[1]["priority"], json!(null));
        assert_eq!(bindings[1]["source"], json!("source_metadata"));
    }

    #[test]
    fn collect_lifecycle_reads_block_defaults_and_last_wins() {
        // No [lifecycle] → unmanaged.
        assert!(collect_lifecycle(&[json!({})]).is_none());
        // [lifecycle] with autostart.
        let lc = collect_lifecycle(&[json!({ "lifecycle": { "autostart": "active" } })]).unwrap();
        assert_eq!(lc["autostart"], json!("active"));
        // [lifecycle] without autostart → defaults to "none" (managed, externally driven).
        let lc = collect_lifecycle(&[json!({ "lifecycle": {} })]).unwrap();
        assert_eq!(lc["autostart"], json!("none"));
        // Last overlay wins.
        let lc = collect_lifecycle(&[
            json!({ "lifecycle": { "autostart": "configure" } }),
            json!({ "lifecycle": { "autostart": "active" } }),
        ])
        .unwrap();
        assert_eq!(lc["autostart"], json!("active"));
    }

    /// Verifies parameter-persistence collection reads block defaults with last-wins semantics.
    #[test]
    fn collect_param_persistence_with_defaults() {
        // No [param_persistence] → no persistence.
        assert!(collect_param_persistence(&[json!({})]).is_none());
        // Empty / missing path → dropped (nothing to persist to).
        assert!(
            collect_param_persistence(&[json!({ "param_persistence": { "backend": "file" } })])
                .is_none()
        );
        // backend defaults to "file".
        let pp = collect_param_persistence(&[json!({
            "param_persistence": { "path": "/var/lib/nros/params" }
        })])
        .unwrap();
        assert_eq!(pp["backend"], json!("file"));
        assert_eq!(pp["path"], json!("/var/lib/nros/params"));
        // Last overlay wins.
        let pp = collect_param_persistence(&[
            json!({ "param_persistence": { "path": "/a" } }),
            json!({ "param_persistence": { "backend": "file", "path": "/b" } }),
        ])
        .unwrap();
        assert_eq!(pp["path"], json!("/b"));
    }

    /// Phase 250 Wave 3 — `[param_services]` collection: presence enables,
    /// `enabled = false` disables (last-wins).
    #[test]
    fn collect_param_services_reads_block() {
        assert!(collect_param_services(&[json!({})]).is_none());
        assert!(collect_param_services(&[json!({ "param_services": {} })]).is_some());
        assert!(
            collect_param_services(&[json!({ "param_services": { "enabled": false } })]).is_none()
        );
        // Last overlay wins: a later disable clears an earlier enable.
        assert!(
            collect_param_services(&[
                json!({ "param_services": {} }),
                json!({ "param_services": { "enabled": false } }),
            ])
            .is_none()
        );
    }

    /// Phase 250 Wave 1 — `[safety]` collection: presence enables (crc default
    /// true), `enabled = false` disables (last-wins), `crc = false` round-trips.
    #[test]
    fn collect_safety_reads_block_with_defaults() {
        // No [safety] → no capability.
        assert!(collect_safety(&[json!({})]).is_none());
        // Bare [safety] table → enabled, crc defaults true.
        let s = collect_safety(&[json!({ "safety": {} })]).unwrap();
        assert_eq!(s["crc"], json!(true));
        // Explicit crc = false round-trips.
        let s = collect_safety(&[json!({ "safety": { "crc": false } })]).unwrap();
        assert_eq!(s["crc"], json!(false));
        // enabled = false disables even when a table is present.
        assert!(collect_safety(&[json!({ "safety": { "enabled": false } })]).is_none());
        // Last overlay wins: a later disable clears an earlier enable.
        assert!(
            collect_safety(&[
                json!({ "safety": { "crc": true } }),
                json!({ "safety": { "enabled": false } }),
            ])
            .is_none()
        );
    }

    #[test]
    fn collect_shared_state_filters_and_merges_overlays() {
        // No [[shared_state]] → empty.
        assert!(collect_shared_state(&[json!({})]).is_empty());
        // Valid entries pass; bad id / zero bytes drop.
        let regions = collect_shared_state(&[json!({
            "shared_state": [
                { "id": "blackboard", "bytes": 256 },
                { "id": "", "bytes": 8 },
                { "id": "zero", "bytes": 0 }
            ]
        })]);
        assert_eq!(regions, vec![json!({ "id": "blackboard", "bytes": 256 })]);
        // Multiple overlays concatenate in order.
        let regions = collect_shared_state(&[
            json!({ "shared_state": [{ "id": "a", "bytes": 4 }] }),
            json!({ "shared_state": [{ "id": "b", "bytes": 8 }] }),
        ]);
        assert_eq!(
            regions,
            vec![
                json!({ "id": "a", "bytes": 4 }),
                json!({ "id": "b", "bytes": 8 })
            ]
        );
    }
}
