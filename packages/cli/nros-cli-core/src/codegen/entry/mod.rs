//! Phase 219.A — Entry-pkg codegen shared module.
//!
//! Lifts the pkg-index walk + launch.xml parse + register-call
//! resolution out of the Rust `nros::main!()` proc-macro (which lives
//! in `packages/core/nros-macros/src/main_macro.rs`) into one place
//! every front-end can call. The three emitters
//! ([`emit_rust`], [`emit_cpp`], [`emit_c`]) consume a single in-memory
//! [`Plan`] IR so the per-language differences stay surface-level.
//!
//! Surface (per phase doc §3.2):
//!
//! ```ignore
//! use nros_cli_core::codegen::entry::{Lang, Plan, plan_from_launch};
//!
//! let plan = plan_from_launch(PlanInput {
//!     workspace: ws.as_path(),
//!     launch_spec: "demo_bringup:system.launch.xml",
//!     board: Some("native".into()),
//!     arg_overrides: vec![],
//! })?;
//! let src = match lang {
//!     Lang::Rust => emit_rust::emit(&plan),               // register-based
//!     Lang::Cpp  => emit_cpp::emit_typed(&plan)?,         // typed (RFC-0043)
//!     Lang::C    => emit_c::emit_typed(&plan)?,           // typed (phase-257)
//! };
//! ```
//!
//! Errors carry enough context that the CLI verb's `eyre::Result`
//! wrapper passes them through verbatim.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};

use eyre::{Context, Result, bail};
use nros_orchestration_ir::{
    CallbackGroupDecl, DEFAULT_TIER, ResolvedTierTable, TierResolveError, resolve_tiers,
};

use crate::{
    launch_parser::{LaunchDescription, NodeSpec, parse_launch_file},
    orchestration::cargo_metadata_schema::{NodeOverride, SystemToml, TierDef},
    pkg_index::build_pkg_index,
};

pub mod emit_c;
pub mod emit_cpp;
pub mod emit_rust;
pub mod metadata;

/// Target language for the emitted TU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Cpp,
    C,
}

impl Lang {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "rust" => Ok(Lang::Rust),
            "cpp" | "c++" | "cxx" => Ok(Lang::Cpp),
            "c" => Ok(Lang::C),
            other => bail!("unknown --lang `{other}` (expected one of: rust, cpp, c)"),
        }
    }
}

/// Caller-supplied inputs to [`plan_from_launch`].
#[derive(Debug)]
pub struct PlanInput<'a> {
    /// Workspace root — the directory holding `src/<pkg>/package.xml`
    /// trees. Typically the dir containing the workspace-root
    /// `CMakeLists.txt` or `Cargo.toml`.
    pub workspace: &'a Path,
    /// `"<bringup_pkg>"` or `"<bringup_pkg>:<file>.launch.xml"`.
    pub launch_spec: &'a str,
    /// Board key (`"native"`, `"freertos"`, …) or an explicit C++
    /// path-like (`"nros::board::NativeBoard"`). For the Rust emitter
    /// this is treated as the `[package.metadata.nros.entry] deploy =`
    /// value; the emitter dispatches to the matching board ZST.
    /// `None` falls back to `"native"` — the only Entry-pkg target
    /// currently supported by the C/C++ surface (Phase 212.L.2).
    pub board: Option<String>,
    /// Caller-supplied launch-arg overrides (forwarded to the parser).
    pub arg_overrides: Vec<(String, String)>,
}

/// Resolved plan handed to one of the three emitters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Board key from [`PlanInput::board`]; `"native"` by default.
    pub board: String,
    /// Per-node entries in launch-file order (top-scope first, then
    /// each `<group>`'s children). Duplicates per `(pkg, exec)` are
    /// preserved — multiple instances of the same Node pkg may occur
    /// in a single launch.
    pub nodes: Vec<PlanNode>,
    /// Absolute paths of every file the plan read. Caller emits these
    /// as `cargo:rerun-if-changed=` / `--depfile` entries for build-
    /// system rebuild correctness.
    pub depfile_paths: Vec<PathBuf>,
    /// Bringup-pkg name from the launch spec (`"demo_bringup"` for
    /// `"demo_bringup:system.launch.xml"`). Surfaced so emitters can
    /// thread it into the generated banner.
    pub bringup: String,
    /// Resolved launch-file path; surfaces in the generated header
    /// banner.
    pub launch_file: PathBuf,
    /// Boot lifecycle autostart from `system.toml [lifecycle].autostart`
    /// (`"none"` | `"configure"` | `"active"`). `None` ⇒ no `[lifecycle]` block. (#117)
    pub lifecycle: Option<String>,
    /// `[param_services]` (or `features=["param_services"]`) enabled — register the
    /// ROS 2 parameter services. (#116)
    pub param_services: bool,
    /// `[safety]` (or `features=["safety"]`) enabled. (#118)
    pub safety: Option<bool>,
    /// Raw `[tiers.*]` for the W4 tier resolver. Empty ⇒ single-tier. (#119)
    pub tiers: BTreeMap<String, TierDef>,
    /// Raw `[[node_overrides]]` for the W4 tier resolver. (#119)
    pub node_overrides: Vec<NodeOverride>,
    /// Phase 269 (W4) — resolved tier table (populated by [`resolve_plan_sched`]).
    /// `None` until the caller invokes the resolver. Emitters check this to gate
    /// sched-context wiring: `None` or `is_single_tier()` → byte-identical output.
    pub resolved_tiers: Option<ResolvedTierTable>,
}

impl Plan {
    /// Phase 211.F — partition a multi-host launch for a single target host.
    ///
    /// Returns a copy of the plan keeping only the nodes that belong on host
    /// `host`: those whose `<node machine="…">` equals `host`, plus every
    /// **unhosted** node (`host == None`) — an unhosted node is shared / runs
    /// everywhere (matches ROS 2, where a node without `machine=` runs on the
    /// local host). The multi-host deploy bakes one entry per host from these
    /// partitions, each deployed to that host's `[deploy.<id>]` target. A
    /// single-host launch (no `machine=` anywhere) is unaffected — every node
    /// is unhosted, so `for_host` returns all of them for any host.
    #[must_use]
    pub fn for_host(&self, host: &str) -> Plan {
        Plan {
            board: self.board.clone(),
            nodes: self
                .nodes
                .iter()
                .filter(|n| match &n.host {
                    Some(h) => h == host,
                    None => true,
                })
                .cloned()
                .collect(),
            depfile_paths: self.depfile_paths.clone(),
            bringup: self.bringup.clone(),
            launch_file: self.launch_file.clone(),
            lifecycle: self.lifecycle.clone(),
            param_services: self.param_services,
            safety: self.safety,
            tiers: self.tiers.clone(),
            node_overrides: self.node_overrides.clone(),
            resolved_tiers: self.resolved_tiers.clone(),
        }
    }

    /// Phase 211.F — the distinct target hosts named across the plan's nodes
    /// (the `machine=` set), sorted + deduped. Empty for a single-host launch.
    /// The multi-host deploy bakes one entry per entry in this set.
    #[must_use]
    pub fn hosts(&self) -> Vec<String> {
        let mut hs: Vec<String> = self.nodes.iter().filter_map(|n| n.host.clone()).collect();
        hs.sort();
        hs.dedup();
        hs
    }
}

/// Phase 211.H (issue #52) — one per-topic QoS override on a node, decomposed
/// from a `qos_overrides.<topic>.<role>.<policy>` launch param. The typed C++
/// entry emitter (`emit_cpp`) bakes these into a static `nros_cpp_qos_override_t[]`
/// + a `node.set_qos_overrides(...)` call before `configure(node)`, so the
/// node's entities honour the override at create time. Fields are the raw
/// decomposed strings; the emitter maps them to the C-ABI `{role, policy, value}`
/// scalar codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QosOverrideSpec {
    /// Resolved topic (e.g. `"/chatter"`).
    pub topic: String,
    /// `"publisher"` or `"subscription"`.
    pub role: String,
    /// `"reliability"` / `"durability"` / `"history"` / `"depth"`.
    pub policy: String,
    /// Raw launch value (e.g. `"best_effort"`, `"transient_local"`, `"5"`).
    pub value: String,
}

/// Decompose a node's launch params into [`QosOverrideSpec`]s. Mirrors the
/// planner's `schema_qos_overrides`: a param named
/// `qos_overrides.<topic>.<role>.<policy>` splits via `rsplitn(3, '.')` (so a
/// `/`-bearing topic survives), and the result is sorted `(topic, role, policy)`
/// for byte-stable codegen. Non-matching params are ignored.
fn qos_overrides_from_params(params: &[crate::launch_parser::ParamSpec]) -> Vec<QosOverrideSpec> {
    const QOS_OVERRIDE_PREFIX: &str = "qos_overrides.";
    let mut out: Vec<QosOverrideSpec> = params
        .iter()
        .filter_map(|p| {
            let rest = p.name.strip_prefix(QOS_OVERRIDE_PREFIX)?;
            // rsplitn(3, '.') → [policy, role, topic]
            let mut parts = rest.rsplitn(3, '.');
            let policy = parts.next()?;
            let role = parts.next()?;
            let topic = parts.next()?;
            if topic.is_empty() || role.is_empty() || policy.is_empty() {
                return None;
            }
            Some(QosOverrideSpec {
                topic: topic.to_string(),
                role: role.to_string(),
                policy: policy.to_string(),
                value: p.value.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.topic, &a.role, &a.policy).cmp(&(&b.topic, &b.role, &b.policy)));
    out
}

/// Collect the non-QoS launch params: everything NOT starting with
/// `qos_overrides.`, in launch-file order. These become `PlanNode.params`.
fn non_qos_params_from_params(params: &[crate::launch_parser::ParamSpec]) -> Vec<(String, String)> {
    const QOS_OVERRIDE_PREFIX: &str = "qos_overrides.";
    params
        .iter()
        .filter(|p| !p.name.starts_with(QOS_OVERRIDE_PREFIX))
        .map(|p| (p.name.clone(), p.value.clone()))
        .collect()
}

/// One Node-pkg invocation in launch order.
///
/// `pkg` is the cargo-style pkg name (sanitised via [`sanitize_pkg`]
/// for symbol-name use). `exec` and `name` come straight from the
/// launch XML; today only `pkg` drives codegen (the per-pkg mangled
/// register symbol is keyed on it), but the `exec` / `name` fields
/// stay on the IR so future `<param>` / `<remap>` routing has them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanNode {
    pub pkg: String,
    pub exec: String,
    pub name: Option<String>,
    pub namespace: Option<String>,
    /// Phase 240.2 (RFC-0043) — fully-qualified C++ component class
    /// (`"talker_pkg::Talker"`), from `nano_ros_node_register(CLASS …)` via the
    /// cmake metadata. Required by the **typed** entry emitter (`emit_cpp_typed`)
    /// which constructs the class; `None` for the legacy register-symbol path.
    pub class_name: Option<String>,
    /// Phase 240.2 — the component class header to `#include`
    /// (`"talker_pkg/Talker.hpp"`). Paired with `class_name`.
    pub class_header: Option<String>,
    /// Phase 240.4 (RFC-0043) — component implementation language from the
    /// cmake metadata (`"c"` / `"cpp"` / `"rust"`). `None` for the launch-only
    /// legacy path. The **typed** entry emitter branches on it: a `"c"` node is
    /// constructed via its C-ABI factory + `configure(node_handle, self)` seam
    /// (`NROS_C_COMPONENT`), a `"cpp"` node via its C++ class + `configure(node)`.
    pub lang: Option<String>,
    /// Phase 242.4 (RFC-0044) — component *shape* from the cmake metadata:
    /// `"rclcpp"` (IS-A-node, ctor-wired — construct-with-handle) or `"configure"`
    /// (RFC-0043 default-construct + `configure(Node&)`). `None` ⇒ `"configure"`
    /// (back-compat). The **typed** entry emitter branches construct on it: an
    /// `"rclcpp"` C++ node is placement-new'd with the executor handle *after*
    /// `nros::init` (the ctor owns the node); a `"configure"` node keeps the
    /// 240.x static-construct-then-`configure(node)` path.
    pub shape: Option<String>,
    /// Phase 211.F — `<node machine="…">` target host (multi-host launch).
    /// `None` for single-host / unhosted nodes. [`Plan::for_host`] partitions a
    /// multi-host launch into per-host bakes: an entry for host `H` keeps nodes
    /// whose `host == Some(H)` plus all unhosted (`None`) nodes.
    pub host: Option<String>,
    /// Phase 211.H (issue #52) — per-topic QoS overrides decomposed from this
    /// node's `qos_overrides.<topic>.<role>.<policy>` launch params. Empty when
    /// none. The typed C++ entry emitter bakes them into a
    /// `node.set_qos_overrides(...)` call before `configure(node)`.
    pub qos_overrides: Vec<QosOverrideSpec>,
    /// Launch `<param name= value=>` initials (NON-qos params; qos ones go to
    /// `qos_overrides`). Preserved in launch-file order. (#116)
    pub params: Vec<(String, String)>,
    /// Per-component callback-group names (from cmake metadata). Empty until the
    /// W4 cmake surface lands. (#119)
    pub callback_groups: Vec<String>,
    /// Resolved sched-context/tier index. `None` until W4 resolves tiers. (#119)
    pub sched_context: Option<u8>,
    /// Phase 273 (RFC-0047 W2) — group → tier bindings from `system.toml
    /// [[component]].group_tiers`. Populated by `plan_from_launch` when the
    /// matching `[[component]]` carries `group_tiers`. Used by `resolve_plan_sched`
    /// to assign each callback group's tier directly, without needing
    /// `[[node_overrides]]`.
    pub group_tiers: BTreeMap<String, String>,
}

impl PlanNode {
    // Phase 258 (Track 2, follow-up) — `register_symbol()` (the dead
    // `__nros_component_<pkg>_register` mangled-symbol string) is gone. The
    // post-257 entries link `__nros_component_<pkg>_install`, not `_register`;
    // nothing consumed this string.

    /// Cmake target name for the static lib the Node pkg's
    /// `nano_ros_node_register()` produces:
    /// `<pkg>_<exec>_component`. The Entry pkg's auto-link
    /// (`nano_ros_entry(... LAUNCH …)`) consumes this string.
    pub fn cmake_link_target(&self) -> String {
        format!("{}_{}_component", sanitize_pkg(&self.pkg), self.exec)
    }
}

/// Phase 266 (W5b/W6) — emit the `NROS_BOOT_CONFIG` static blob (C/C++ shared helper).
///
/// For a **single-node** plan the blob bakes the launch node name into
/// `.node_name` with `NROS_BOOT_SET_NODE_NAME` set; a post-link tool (or the
/// runner's inline call to `nros_boot_config_node_name`) can read it back.
/// For a **multi-node** plan (or when the single node has no resolvable name)
/// all fields are zero / unset — `nros_boot_config_node_name` returns NULL and
/// the runner falls back to the unified `"node"` default.
///
/// # Errors
///
/// Returns `Err` if the resolved node name exceeds 63 bytes (the
/// `nros_baked_boot_config.node_name` C field is `char node_name[64]`, which
/// must hold the string **and** a NUL terminator). The caller receives a clear
/// diagnostic rather than a confusing C-compiler array-initialiser error.
///
/// Callers must have already emitted `#include <nros/boot_config.h>`.
pub fn emit_boot_config_static(out: &mut String, plan: &Plan) -> Result<(), String> {
    use std::fmt::Write as _;
    let (set_flags, node_name) = if plan.nodes.len() == 1 {
        let n = &plan.nodes[0];
        let raw = n.name.as_deref().unwrap_or(&n.exec);
        // Guard: the C field is `char node_name[64]` — 63 usable bytes + NUL.
        if raw.len() > 63 {
            return Err(format!(
                "node name '{}' is {} bytes; the .nros_boot_config node_name field holds at most 63 bytes + NUL",
                raw,
                raw.len(),
            ));
        }
        // Escape the literal for embedding in a C string — backslash and quote.
        let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
        ("NROS_BOOT_SET_NODE_NAME", escaped)
    } else {
        ("0", String::new())
    };
    let _ = writeln!(
        out,
        "/* Phase 266 (RFC-0045) — baked boot config: post-link readable + session name. */\n\
         static const struct nros_baked_boot_config NROS_BOOT_CONFIG\n\
         #if defined(__GNUC__) || defined(__clang__)\n\
             __attribute__((section(\".nros_boot_config\"), used))\n\
         #endif\n\
             = {{\n\
             .magic      = NROS_BOOT_CONFIG_MAGIC,\n\
             .version    = NROS_BOOT_CONFIG_VERSION,\n\
             .set_flags  = {set_flags},\n\
             .domain_id  = 0,\n\
             .node_name  = \"{node_name}\",\n\
             .locator    = \"\",\n\
             .namespace_ = \"\",\n\
         }};",
    );
    Ok(())
}

/// Sanitise a pkg name into a valid identifier (`-` → `_`).
///
/// Mirrors the rule the Rust `nros::node!()` macro and the cmake fn
/// `nano_ros_node_register()` already apply (see
/// `packages/core/nros-macros/src/main_macro.rs::pkg_to_crate_ident`).
pub fn sanitize_pkg(pkg: &str) -> String {
    let mut out = String::with_capacity(pkg.len());
    for c in pkg.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

/// Walk the workspace pkg-index, locate the bringup pkg, parse the
/// launch XML, and lower the result into a [`Plan`].
///
/// Errors carry enough context that the CLI verb's `eyre::Result`
/// wrapper passes them through verbatim.
pub fn plan_from_launch(input: PlanInput<'_>) -> Result<Plan> {
    let pkg_index = build_pkg_index(input.workspace)
        .with_context(|| format!("build pkg-index from `{}`", input.workspace.display()))?;

    let (bringup_name, file_override) = match input.launch_spec.split_once(':') {
        Some((b, f)) => (b.trim().to_string(), Some(f.trim().to_string())),
        None => (input.launch_spec.trim().to_string(), None),
    };
    if bringup_name.is_empty() {
        bail!(
            "empty bringup pkg name in launch spec `{}`",
            input.launch_spec
        );
    }

    let bringup_dir = pkg_index
        .resolve_pkg(&bringup_name)
        .with_context(|| format!("resolve bringup pkg `{bringup_name}`"))?
        .to_path_buf();

    // Pull every package.xml the index walked into the depfile list.
    let mut depfile_paths: Vec<PathBuf> = Vec::new();
    for (_, pkg_dir) in pkg_index.pkgs() {
        depfile_paths.push(pkg_dir.join("package.xml"));
    }

    // Parse bringup's system.toml when present — provides [lifecycle],
    // [param_services], [safety], [tiers.*], [[node_overrides]], and
    // [system].default_launch. Absent → all Plan feature fields stay at defaults.
    let system_toml_path = bringup_dir.join("system.toml");
    let parsed_system: Option<SystemToml> = if system_toml_path.exists() {
        depfile_paths.push(system_toml_path.clone());
        let raw = std::fs::read_to_string(&system_toml_path)
            .with_context(|| format!("read `{}`", system_toml_path.display()))?;
        Some(
            toml::from_str::<SystemToml>(&raw)
                .with_context(|| format!("parse `{}`", system_toml_path.display()))?,
        )
    } else {
        None
    };

    // Resolve the launch filename. With no `:file` override, consult
    // bringup's `system.toml [system] default_launch`; fall back to
    // `system.launch.xml` (matches the Rust proc-macro shape).
    let launch_filename = match file_override {
        Some(s) => s,
        None => parsed_system
            .as_ref()
            .and_then(|s| s.system.default_launch.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| "system.launch.xml".to_string()),
    };
    let launch_path = bringup_dir.join("launch").join(&launch_filename);
    if !launch_path.exists() {
        bail!("launch file not found: `{}`", launch_path.display());
    }
    depfile_paths.push(launch_path.clone());

    let desc: LaunchDescription = parse_launch_file(&launch_path, &pkg_index, &input.arg_overrides)
        .with_context(|| format!("parse launch file `{}`", launch_path.display()))?;

    // Walk every `<node>` (top-level + inside groups) and lower to a
    // PlanNode. Order matches the source XML (top-scope first, then
    // each group's children).
    let mut nodes: Vec<PlanNode> = Vec::new();
    let push_node = |n: &NodeSpec, out: &mut Vec<PlanNode>| {
        out.push(PlanNode {
            pkg: n.pkg.clone(),
            exec: n.exec.clone(),
            name: n.name.clone(),
            namespace: n.namespace.clone(),
            // Phase 240.2 — the launch path doesn't carry the C++ class/header
            // yet (threaded from cmake metadata in 240.2b); the legacy
            // register-symbol emitters ignore these.
            class_name: None,
            class_header: None,
            lang: None,
            shape: None,
            host: n.machine.clone(),
            qos_overrides: qos_overrides_from_params(&n.params),
            params: non_qos_params_from_params(&n.params),
            callback_groups: Vec::new(),
            sched_context: None,
            group_tiers: BTreeMap::new(),
        });
    };
    for n in &desc.nodes {
        push_node(n, &mut nodes);
    }
    for g in &desc.groups {
        for n in &g.nodes {
            push_node(n, &mut nodes);
        }
    }

    if nodes.is_empty() {
        bail!(
            "launch file `{}` has no `<node>` entries — nothing to register",
            launch_path.display()
        );
    }

    // Phase 273 (W2) — populate each PlanNode.group_tiers from the matching
    // [[component]] entry in system.toml (RFC-0047: group→tier is deployment config).
    // Matching: PlanNode name (or exec) vs SystemComponentEntry.name.
    if let Some(ref sys) = parsed_system {
        let gt_by_name: BTreeMap<&str, &BTreeMap<String, String>> = sys
            .components
            .iter()
            .filter(|c| !c.group_tiers.is_empty())
            .map(|c| (c.name.as_str(), &c.group_tiers))
            .collect();
        if !gt_by_name.is_empty() {
            for n in &mut nodes {
                let node_name = n.name.as_deref().unwrap_or(n.exec.as_str());
                if let Some(gt) = gt_by_name.get(node_name) {
                    n.group_tiers = (*gt).clone();
                }
            }
        }
    }

    // Sort + dedup the depfile entries — pkg-index revisits + sibling
    // `<include>`s can list a single file twice.
    depfile_paths.sort();
    depfile_paths.dedup();

    let board = input.board.unwrap_or_else(|| "native".to_string());

    let lifecycle = parsed_system
        .as_ref()
        .and_then(|s| s.lifecycle.as_ref())
        .map(|l| l.autostart.clone());
    let param_services = parsed_system
        .as_ref()
        .is_some_and(|s| s.capability_enabled("param_services"));
    let safety = parsed_system.as_ref().and_then(|s| {
        s.capability_enabled("safety")
            .then(|| s.safety.as_ref().map(|sf| sf.crc).unwrap_or(true))
    });
    let tiers = parsed_system
        .as_ref()
        .map(|s| s.tiers.clone())
        .unwrap_or_default();
    let node_overrides = parsed_system
        .as_ref()
        .map(|s| s.node_overrides.clone())
        .unwrap_or_default();

    Ok(Plan {
        board,
        nodes,
        depfile_paths,
        bringup: bringup_name,
        launch_file: launch_path,
        lifecycle,
        param_services,
        safety,
        tiers,
        node_overrides,
        resolved_tiers: None,
    })
}

/// R1-N2 (RFC-0052 / phase-296 W4.1) — build a [`Plan`] from a resolved
/// SystemModel instead of parsing a launch file. The model is the
/// canonical artifact: structure supplies the node list (params included
/// — the embedded image has no record.json), execution supplies tiers,
/// group bindings, capability features, and per-node deploy facts.
///
/// Board slice: with deploy entries present, `board == "native"/"posix"`
/// keeps `linux`-targeted nodes and any other board key keeps
/// `mcu:<that board>` nodes; a model WITHOUT deploy entries deploys every
/// node (single-image case). An empty slice is a hard error — a bake for
/// a board no node targets is a placement bug, not an empty entry.
pub fn plan_from_model(model_path: &Path, board: Option<String>) -> Result<Plan> {
    use crate::orchestration::model_ingest;
    use ros_launch_manifest_model::{ParamValue, Target};

    let model = model_ingest::load_model(model_path)?;
    let board = board.unwrap_or_else(|| "native".to_string());
    let target_rtos = board_to_rtos(&board).to_string();

    let keep = |fqn: &str| -> bool {
        let Some(dep) = model.execution.deploy.get(fqn) else {
            return model.execution.deploy.is_empty();
        };
        match (&dep.target, board.as_str()) {
            (Target::Linux, "native" | "posix") => true,
            (Target::Mcu { board: b }, key) => b == key,
            _ => false,
        }
    };

    let mut nodes: Vec<PlanNode> = Vec::new();
    for (fqn, inst) in &model.structure.nodes {
        if !keep(fqn) {
            continue;
        }
        let bare = fqn.rsplit('/').next().unwrap_or(fqn).to_string();
        let namespace = {
            let ns = &fqn[..fqn.len() - bare.len()];
            let ns = ns.trim_end_matches('/');
            if ns.is_empty() {
                "/".to_string()
            } else {
                ns.to_string()
            }
        };
        let exec = inst
            .exec
            .clone()
            .or_else(|| {
                // Library-component node: the plugin (class) names it; the
                // typed emitters resolve the real class/header from cmake
                // metadata by (pkg, exec/name) as usual.
                inst.plugin
                    .as_deref()
                    .map(|p| p.rsplit("::").next().unwrap_or(p).to_string())
            })
            .ok_or_else(|| eyre::eyre!("model node '{fqn}' has neither exec nor plugin"))?;
        let params: Vec<(String, String)> = inst
            .params
            .iter()
            .map(|(k, v)| {
                let s = match v {
                    ParamValue::Bool(b) => b.to_string(),
                    ParamValue::Int(i) => i.to_string(),
                    ParamValue::Float(f) => f.to_string(),
                    ParamValue::Str(s) => s.clone(),
                    ParamValue::StrList(l) => l.join(","),
                };
                (k.clone(), s)
            })
            .collect();
        // Group→tier from the model's resolved bindings (`<fqn>/<group>`).
        let mut group_tiers: BTreeMap<String, String> = BTreeMap::new();
        let prefix = format!("{fqn}/");
        for (key, tier) in &model.execution.bindings {
            if let Some(group) = key.strip_prefix(&prefix) {
                group_tiers.insert(group.to_string(), tier.clone());
            }
        }
        nodes.push(PlanNode {
            pkg: inst.pkg.clone().unwrap_or_default(),
            exec,
            name: Some(bare),
            namespace: Some(namespace),
            class_name: None,
            class_header: None,
            lang: None,
            shape: None,
            host: model.execution.deploy.get(fqn).and_then(|d| d.host.clone()),
            qos_overrides: Vec::new(),
            params,
            callback_groups: Vec::new(),
            sched_context: None,
            group_tiers,
        });
    }
    if nodes.is_empty() {
        bail!(
            "SystemModel `{}` places no nodes on board `{board}` — check \
             execution.deploy targets",
            model_path.display()
        );
    }

    let tiers: BTreeMap<String, TierDef> = model
        .execution
        .tiers
        .iter()
        .map(|(name, t)| {
            (
                name.clone(),
                crate::orchestration::model_ingest::tier_from_model(t, &target_rtos),
            )
        })
        .collect();

    let lifecycle = model
        .structure
        .nodes
        .values()
        .find_map(|n| n.lifecycle_autostart)
        .map(|a| {
            match a {
                ros_launch_manifest_model::Autostart::None => "none",
                ros_launch_manifest_model::Autostart::Configure => "configure",
                ros_launch_manifest_model::Autostart::Active => "active",
            }
            .to_string()
        });
    let features = &model.execution.features;
    let param_services = features.iter().any(|f| f == "param_services");
    let safety = features.iter().any(|f| f == "safety").then_some(true);

    Ok(Plan {
        board,
        nodes,
        depfile_paths: vec![model_path.to_path_buf()],
        bringup: "system-model".to_string(),
        launch_file: model_path.to_path_buf(),
        lifecycle,
        param_services,
        safety,
        tiers,
        node_overrides: Vec::new(),
        resolved_tiers: None,
    })
}

/// Phase 269 (W4) — derive the RTOS key recognised by [`resolve_tiers`] from a
/// board deploy key. The mapping mirrors the `rtos_spec` function in
/// `nros-orchestration-ir` (`"posix" | "native"`, `"freertos"`, …).
pub fn board_to_rtos(board: &str) -> &str {
    match board {
        "native" | "posix" => "posix",
        b if b.starts_with("freertos") || b.contains("freertos") => "freertos",
        b if b.starts_with("zephyr") || b.contains("zephyr") => "zephyr",
        b if b.starts_with("nuttx") || b.contains("nuttx") => "nuttx",
        b if b.starts_with("threadx") || b.contains("threadx") => "threadx",
        _ => "posix",
    }
}

/// Phase 269 (W4) — resolve `[tiers.*]` + `[[node_overrides]]` + per-node
/// `callback_groups` (from cmake metadata) into a [`ResolvedTierTable`] and
/// stamp each [`PlanNode::sched_context`] with its 0-based tier index
/// (highest-priority-first order from the resolver).
///
/// Must be called AFTER [`metadata::enrich_plan`] so that
/// [`PlanNode::callback_groups`] is populated. The caller supplies the RTOS
/// key (use [`board_to_rtos`] to derive it from `plan.board`).
///
/// No-op (returns `Ok(())`) when both `plan.tiers` and `plan.node_overrides`
/// are empty and no node declares callback groups — the guard keeps
/// single-tier entries byte-identical.
pub fn resolve_plan_sched(plan: &mut Plan, target_rtos: &str) -> Result<()> {
    let has_groups = plan
        .nodes
        .iter()
        .any(|n| !n.callback_groups.is_empty() || !n.group_tiers.is_empty());
    if plan.tiers.is_empty() && plan.node_overrides.is_empty() && !has_groups {
        return Ok(());
    }

    // Component instance names from the launch (match [[node_overrides]].name).
    let component_names: BTreeSet<&str> = plan
        .nodes
        .iter()
        .map(|n| n.name.as_deref().unwrap_or(n.exec.as_str()))
        .collect();

    // Per-node callback group declarations. Phase 273 (W2): when the node carries
    // `group_tiers` from system.toml [[component]], use those tiers directly instead
    // of defaulting to "default" (which required [[node_overrides]] to reassign).
    // Fallback: group ID with DEFAULT_TIER (old path, [[node_overrides]] still work).
    // If callback_groups is empty but group_tiers is set, synthesize from group_tiers.
    let mut callback_groups_map: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::new();
    for n in &plan.nodes {
        let node_name = n.name.as_deref().unwrap_or(n.exec.as_str()).to_string();
        let decls: Vec<CallbackGroupDecl> = if !n.callback_groups.is_empty() {
            // cmake-declared groups (via enrich_plan): look up tier from group_tiers.
            n.callback_groups
                .iter()
                .map(|g| {
                    let tier = n
                        .group_tiers
                        .get(g)
                        .map(|t| t.as_str())
                        .unwrap_or(DEFAULT_TIER);
                    CallbackGroupDecl {
                        id: g.clone(),
                        r#type: "MutuallyExclusive".to_string(),
                        tier: tier.to_string(),
                    }
                })
                .collect()
        } else if !n.group_tiers.is_empty() {
            // No cmake callback_groups yet; synthesize from system.toml group_tiers.
            n.group_tiers
                .iter()
                .map(|(id, tier)| CallbackGroupDecl {
                    id: id.clone(),
                    r#type: "MutuallyExclusive".to_string(),
                    tier: tier.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
        if !decls.is_empty() {
            callback_groups_map.insert(node_name, decls);
        }
    }

    let table = resolve_tiers(
        &plan.tiers,
        &plan.node_overrides,
        &component_names,
        &callback_groups_map,
        target_rtos,
    )
    .map_err(|e: TierResolveError| eyre::eyre!("tier resolution failed: {e}"))?;

    // Stamp PlanNode.sched_context with the 0-based index into the ordered tier list
    // (highest-priority-first). Nodes not assigned to any tier keep `sched_context = None`.
    let mut tier_map: HashMap<String, u8> = HashMap::new();
    for (i, tier) in table.tiers.iter().enumerate() {
        for (node_name, _group) in &tier.members {
            tier_map.insert(node_name.clone(), i as u8);
        }
    }
    for n in &mut plan.nodes {
        let node_name = n.name.as_deref().unwrap_or(n.exec.as_str());
        if let Some(&idx) = tier_map.get(node_name) {
            n.sched_context = Some(idx);
        }
    }

    plan.resolved_tiers = Some(table);
    Ok(())
}

/// Write the depfile in GNU-make `target: dep1 dep2 …` form so cmake's
/// `CMAKE_CONFIGURE_DEPENDS` / make-style consumers ingest it directly.
///
/// The "target" line is the generated TU path (or whatever the caller
/// supplies); each dep is one absolute path on its own continuation
/// line so newline-quoting + path escaping stays trivial.
pub fn write_depfile(target: &Path, deps: &[PathBuf], depfile: &Path) -> Result<()> {
    let mut out = String::new();
    out.push_str(&escape_make(&target.display().to_string()));
    out.push(':');
    for dep in deps {
        out.push_str(" \\\n    ");
        out.push_str(&escape_make(&dep.display().to_string()));
    }
    out.push('\n');
    if let Some(parent) = depfile.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create depfile parent `{}`", parent.display()))?;
    }
    std::fs::write(depfile, out)
        .with_context(|| format!("write depfile `{}`", depfile.display()))?;
    Ok(())
}

/// Make-format target/dep escape: spaces → `\ `, `#` → `\#`.
fn escape_make(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("\\ "),
            '#' => out.push_str("\\#"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_pkg_dash_to_underscore() {
        assert_eq!(sanitize_pkg("talker-pkg"), "talker_pkg");
        assert_eq!(sanitize_pkg("a.b/c"), "a_b_c");
        assert_eq!(sanitize_pkg("plain_pkg"), "plain_pkg");
    }

    #[test]
    fn cmake_link_target_uses_mangled_pkg() {
        let n = PlanNode {
            pkg: "talker-pkg".into(),
            exec: "talker".into(),
            name: None,
            namespace: None,
            class_name: None,
            class_header: None,
            lang: None,
            shape: None,
            host: None,
            qos_overrides: Vec::new(),
            params: Vec::new(),
            callback_groups: Vec::new(),
            sched_context: None,
            group_tiers: BTreeMap::new(),
        };
        assert_eq!(n.cmake_link_target(), "talker_pkg_talker_component");
    }

    /// Phase 211.F — `Plan::for_host` keeps a host's own nodes + all unhosted
    /// (shared) nodes; `hosts()` returns the distinct `machine=` set.
    #[test]
    fn plan_for_host_partitions_by_machine() {
        let node = |pkg: &str, host: Option<&str>| PlanNode {
            pkg: pkg.into(),
            exec: pkg.into(),
            name: None,
            namespace: None,
            class_name: None,
            class_header: None,
            lang: None,
            shape: None,
            host: host.map(str::to_string),
            qos_overrides: Vec::new(),
            params: Vec::new(),
            callback_groups: Vec::new(),
            sched_context: None,
            group_tiers: BTreeMap::new(),
        };
        let plan = Plan {
            board: "native".into(),
            nodes: vec![
                node("sim", Some("workstation")),
                node("ctrl", Some("jetson")),
                node("shared", None),
            ],
            depfile_paths: vec![],
            bringup: "demo".into(),
            launch_file: std::path::PathBuf::from("/tmp/x.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers: Default::default(),
            node_overrides: Vec::new(),
            resolved_tiers: None,
        };

        assert_eq!(
            plan.hosts(),
            vec!["jetson".to_string(), "workstation".to_string()]
        );

        // workstation entry: its own node + the shared (unhosted) one.
        let ws = plan.for_host("workstation");
        let ws_pkgs: Vec<&str> = ws.nodes.iter().map(|n| n.pkg.as_str()).collect();
        assert_eq!(ws_pkgs, vec!["sim", "shared"]);

        // jetson entry: its own node + shared.
        let jetson = plan.for_host("jetson");
        let jetson_pkgs: Vec<&str> = jetson.nodes.iter().map(|n| n.pkg.as_str()).collect();
        assert_eq!(jetson_pkgs, vec!["ctrl", "shared"]);

        // An unknown host still gets the shared nodes (never empty for a launch
        // with shared nodes).
        assert_eq!(plan.for_host("nope").nodes.len(), 1);
    }

    /// Phase 211.H (issue #52) — `qos_overrides.<topic>.<role>.<policy>` params
    /// decompose into sorted `QosOverrideSpec`s; non-matching params are ignored.
    #[test]
    fn qos_overrides_decompose_from_params() {
        use crate::launch_parser::ParamSpec;
        let params = vec![
            ParamSpec {
                name: "qos_overrides./chatter.publisher.reliability".into(),
                value: "best_effort".into(),
            },
            ParamSpec {
                name: "use_sim_time".into(),
                value: "true".into(),
            },
            ParamSpec {
                name: "qos_overrides./chatter.subscription.durability".into(),
                value: "transient_local".into(),
            },
        ];
        let got = qos_overrides_from_params(&params);
        assert_eq!(got.len(), 2);
        // sorted (topic, role, policy): publisher before subscription.
        assert_eq!(got[0].role, "publisher");
        assert_eq!(got[0].policy, "reliability");
        assert_eq!(got[0].topic, "/chatter");
        assert_eq!(got[0].value, "best_effort");
        assert_eq!(got[1].role, "subscription");
        assert_eq!(got[1].policy, "durability");
    }

    #[test]
    fn lang_parse() {
        assert_eq!(Lang::parse("rust").unwrap(), Lang::Rust);
        assert_eq!(Lang::parse("cpp").unwrap(), Lang::Cpp);
        assert_eq!(Lang::parse("c++").unwrap(), Lang::Cpp);
        assert_eq!(Lang::parse("c").unwrap(), Lang::C);
        assert!(Lang::parse("python").is_err());
    }

    #[test]
    fn write_depfile_emits_make_format() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("gen.cpp");
        let dep_a = tmp.path().join("a.xml");
        let dep_b = tmp.path().join("dir with space/b.xml");
        let depfile = tmp.path().join("gen.d");
        write_depfile(&target, &[dep_a.clone(), dep_b.clone()], &depfile).unwrap();
        let body = std::fs::read_to_string(&depfile).unwrap();
        assert!(body.starts_with(&format!("{}:", target.display())));
        assert!(body.contains(&format!("{}", dep_a.display())));
        // Space in dep_b should be escaped.
        assert!(body.contains("dir\\ with\\ space"));
    }

    /// Helper that builds a single-node [`Plan`] with the given node name.
    fn single_node_plan(name: &str) -> Plan {
        Plan {
            board: "native".into(),
            nodes: vec![PlanNode {
                pkg: "test_pkg".into(),
                exec: name.into(),
                name: None,
                namespace: None,
                class_name: None,
                class_header: None,
                lang: None,
                shape: None,
                host: None,
                qos_overrides: Vec::new(),
                params: Vec::new(),
                callback_groups: Vec::new(),
                sched_context: None,
                group_tiers: BTreeMap::new(),
            }],
            depfile_paths: vec![],
            bringup: "demo".into(),
            launch_file: std::path::PathBuf::from("/tmp/x.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers: Default::default(),
            node_overrides: Vec::new(),
            resolved_tiers: None,
        }
    }

    /// Phase 266 (W6) — a node name of exactly 63 bytes must succeed (fits in
    /// `char node_name[64]` with one byte left for the NUL terminator).
    #[test]
    fn boot_config_node_name_63_bytes_ok() {
        let name = "a".repeat(63);
        assert_eq!(name.len(), 63);
        let plan = single_node_plan(&name);
        let mut out = String::new();
        assert!(
            emit_boot_config_static(&mut out, &plan).is_ok(),
            "63-byte name should be accepted"
        );
        assert!(
            out.contains(&name),
            "emitted output must contain the node name"
        );
    }

    /// Phase 266 (W6) — a node name of 64 bytes must be rejected with a clear
    /// error message (the C field only holds 63 usable bytes + NUL).
    #[test]
    fn boot_config_node_name_64_bytes_err() {
        let name = "b".repeat(64);
        assert_eq!(name.len(), 64);
        let plan = single_node_plan(&name);
        let mut out = String::new();
        let err =
            emit_boot_config_static(&mut out, &plan).expect_err("64-byte name must be rejected");
        assert!(
            err.contains("64 bytes"),
            "error should mention the byte count; got: {err}"
        );
        assert!(
            err.contains("node_name"),
            "error should mention the field name; got: {err}"
        );
        assert!(
            err.contains("63 bytes"),
            "error should mention the 63-byte limit; got: {err}"
        );
    }

    /// Phase 269 (W0) — non-QoS params are baked into PlanNode.params; qos_overrides.* params
    /// go to qos_overrides, not to params.
    #[test]
    fn non_qos_params_split_from_qos_params() {
        use crate::launch_parser::ParamSpec;
        let params = vec![
            ParamSpec {
                name: "p".into(),
                value: "v".into(),
            },
            ParamSpec {
                name: "qos_overrides./chatter.publisher.reliability".into(),
                value: "best_effort".into(),
            },
            ParamSpec {
                name: "count".into(),
                value: "42".into(),
            },
        ];
        let non_qos = non_qos_params_from_params(&params);
        assert_eq!(non_qos.len(), 2);
        assert_eq!(non_qos[0], ("p".into(), "v".into()));
        assert_eq!(non_qos[1], ("count".into(), "42".into()));
    }

    /// Phase 269 (W0) — a system.toml with [lifecycle], [safety], [param_services],
    /// and [tiers.*] yields a Plan with the matching feature fields.
    #[test]
    fn plan_from_launch_reads_system_toml_feature_fields() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();

        // Build a minimal workspace: src/<pkg>/package.xml trees.
        let bringup_dir = tmp.path().join("src").join("demo_bringup");
        std::fs::create_dir_all(bringup_dir.join("launch")).unwrap();
        std::fs::write(
            bringup_dir.join("package.xml"),
            r#"<?xml version="1.0"?><package format="3"><name>demo_bringup</name></package>"#,
        )
        .unwrap();
        std::fs::write(
            bringup_dir.join("launch").join("system.launch.xml"),
            r#"<launch><node pkg="talker_pkg" exec="talker"/></launch>"#,
        )
        .unwrap();
        let talker_dir = tmp.path().join("src").join("talker_pkg");
        std::fs::create_dir_all(&talker_dir).unwrap();
        std::fs::write(
            talker_dir.join("package.xml"),
            r#"<?xml version="1.0"?><package format="3"><name>talker_pkg</name></package>"#,
        )
        .unwrap();

        // Write system.toml with all feature fields.
        std::fs::write(
            bringup_dir.join("system.toml"),
            r#"[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[lifecycle]
autostart = "active"

[safety]

[param_services]

[tiers.rt]
"#,
        )
        .unwrap();

        let plan = plan_from_launch(PlanInput {
            workspace: tmp.path(),
            launch_spec: "demo_bringup",
            board: None,
            arg_overrides: vec![],
        })
        .expect("plan_from_launch should succeed");

        assert_eq!(plan.lifecycle.as_deref(), Some("active"));
        assert!(plan.param_services, "param_services should be enabled");
        assert_eq!(
            plan.safety,
            Some(true),
            "safety should be Some(true) (crc=true default)"
        );
        assert!(plan.tiers.contains_key("rt"), "tiers should contain 'rt'");
    }

    /// Phase 269 (W0) — a launch without system.toml leaves all feature fields at defaults,
    /// keeping the Plan byte-identical for pre-W0 callers.
    #[test]
    fn plan_from_launch_no_system_toml_yields_defaults() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();

        let bringup_dir = tmp.path().join("src").join("demo_bringup");
        std::fs::create_dir_all(bringup_dir.join("launch")).unwrap();
        std::fs::write(
            bringup_dir.join("package.xml"),
            r#"<?xml version="1.0"?><package format="3"><name>demo_bringup</name></package>"#,
        )
        .unwrap();
        std::fs::write(
            bringup_dir.join("launch").join("system.launch.xml"),
            r#"<launch><node pkg="talker_pkg" exec="talker"><param name="p" value="v"/></node></launch>"#,
        )
        .unwrap();
        let talker_dir = tmp.path().join("src").join("talker_pkg");
        std::fs::create_dir_all(&talker_dir).unwrap();
        std::fs::write(
            talker_dir.join("package.xml"),
            r#"<?xml version="1.0"?><package format="3"><name>talker_pkg</name></package>"#,
        )
        .unwrap();

        let plan = plan_from_launch(PlanInput {
            workspace: tmp.path(),
            launch_spec: "demo_bringup",
            board: None,
            arg_overrides: vec![],
        })
        .expect("plan_from_launch should succeed without system.toml");

        assert!(plan.lifecycle.is_none());
        assert!(!plan.param_services);
        assert!(plan.safety.is_none());
        assert!(plan.tiers.is_empty());
        assert!(plan.node_overrides.is_empty());
        // Non-QoS params baked into PlanNode.params.
        assert_eq!(
            plan.nodes[0].params,
            vec![("p".to_string(), "v".to_string())]
        );
    }

    /// Phase 269 (W4) — resolve_plan_sched assigns PlanNode.sched_context from tiers +
    /// node_overrides + callback_groups. A 2-tier plan with node overrides yields
    /// correct sched_context indices (highest-priority-first: high=0, low=1).
    #[test]
    fn resolve_plan_sched_stamps_sched_context_indices() {
        use crate::orchestration::cargo_metadata_schema::{
            CallbackGroupOverride, NodeOverride, TierDef, TierRtosSpec,
        };
        use std::path::PathBuf;

        let high_tier = TierDef {
            spin_period_us: Some(10_000),
            posix: Some(TierRtosSpec {
                priority: 80,
                stack_bytes: None,
                preempt_threshold: None,
                sched_class: None,
            }),
            ..Default::default()
        };
        let low_tier = TierDef {
            spin_period_us: Some(100_000),
            posix: Some(TierRtosSpec {
                priority: 10,
                stack_bytes: None,
                preempt_threshold: None,
                sched_class: None,
            }),
            ..Default::default()
        };
        let mut tiers = std::collections::BTreeMap::new();
        tiers.insert("high".to_string(), high_tier);
        tiers.insert("low".to_string(), low_tier);

        let node_overrides = vec![
            NodeOverride {
                name: "ctrl".to_string(),
                callback_groups: vec![CallbackGroupOverride {
                    id: "ctrl_grp".to_string(),
                    tier: "high".to_string(),
                }],
            },
            NodeOverride {
                name: "telem".to_string(),
                callback_groups: vec![CallbackGroupOverride {
                    id: "telem_grp".to_string(),
                    tier: "low".to_string(),
                }],
            },
        ];

        let mut plan = Plan {
            board: "native".into(),
            nodes: vec![
                PlanNode {
                    pkg: "ctrl_pkg".into(),
                    exec: "ctrl".into(),
                    name: Some("ctrl".into()),
                    namespace: None,
                    class_name: None,
                    class_header: None,
                    lang: Some("c".into()),
                    shape: None,
                    host: None,
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    callback_groups: vec!["ctrl_grp".into()],
                    sched_context: None,
                    group_tiers: BTreeMap::new(),
                },
                PlanNode {
                    pkg: "telem_pkg".into(),
                    exec: "telem".into(),
                    name: Some("telem".into()),
                    namespace: None,
                    class_name: None,
                    class_header: None,
                    lang: Some("c".into()),
                    shape: None,
                    host: None,
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    callback_groups: vec!["telem_grp".into()],
                    sched_context: None,
                    group_tiers: BTreeMap::new(),
                },
            ],
            depfile_paths: vec![],
            bringup: "demo".into(),
            launch_file: PathBuf::from("/tmp/x.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers,
            node_overrides,
            resolved_tiers: None,
        };

        resolve_plan_sched(&mut plan, "posix").expect("resolve_plan_sched should succeed");

        // resolved_tiers populated.
        assert!(plan.resolved_tiers.is_some(), "resolved_tiers must be set");
        let table = plan.resolved_tiers.as_ref().unwrap();
        assert!(
            !table.is_single_tier(),
            "two-tier plan must not be single-tier"
        );
        // highest-priority-first: high (80) = idx 0, low (10) = idx 1.
        assert_eq!(table.tiers[0].name, "high");
        assert_eq!(table.tiers[1].name, "low");
        // PlanNode.sched_context stamped correctly.
        assert_eq!(
            plan.nodes[0].sched_context,
            Some(0),
            "ctrl (high tier) must get sched_context=0"
        );
        assert_eq!(
            plan.nodes[1].sched_context,
            Some(1),
            "telem (low tier) must get sched_context=1"
        );
    }

    /// Phase 273 (W2) — resolve_plan_sched uses PlanNode.group_tiers to assign
    /// callback-group tiers directly from system.toml, without needing
    /// [[node_overrides]]. A 2-tier plan with group_tiers set on each node
    /// yields the same sched_context stamping as the node_overrides path.
    #[test]
    fn resolve_plan_sched_uses_group_tiers_directly() {
        use crate::orchestration::cargo_metadata_schema::{TierDef, TierRtosSpec};
        use std::path::PathBuf;

        let mut tiers = BTreeMap::new();
        tiers.insert(
            "high".to_string(),
            TierDef {
                spin_period_us: Some(10_000),
                posix: Some(TierRtosSpec {
                    priority: 80,
                    stack_bytes: None,
                    preempt_threshold: None,
                    sched_class: None,
                }),
                ..Default::default()
            },
        );
        tiers.insert(
            "low".to_string(),
            TierDef {
                spin_period_us: Some(100_000),
                posix: Some(TierRtosSpec {
                    priority: 10,
                    stack_bytes: None,
                    preempt_threshold: None,
                    sched_class: None,
                }),
                ..Default::default()
            },
        );

        let mut ctrl_gt = BTreeMap::new();
        ctrl_gt.insert("ctrl_grp".to_string(), "high".to_string());
        let mut telem_gt = BTreeMap::new();
        telem_gt.insert("telem_grp".to_string(), "low".to_string());

        let mut plan = Plan {
            board: "native".into(),
            nodes: vec![
                PlanNode {
                    pkg: "ctrl_pkg".into(),
                    exec: "ctrl".into(),
                    name: Some("ctrl".into()),
                    namespace: None,
                    class_name: None,
                    class_header: None,
                    lang: Some("c".into()),
                    shape: None,
                    host: None,
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    callback_groups: vec!["ctrl_grp".into()],
                    sched_context: None,
                    group_tiers: ctrl_gt,
                },
                PlanNode {
                    pkg: "telem_pkg".into(),
                    exec: "telem".into(),
                    name: Some("telem".into()),
                    namespace: None,
                    class_name: None,
                    class_header: None,
                    lang: Some("c".into()),
                    shape: None,
                    host: None,
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    callback_groups: vec!["telem_grp".into()],
                    sched_context: None,
                    group_tiers: telem_gt,
                },
            ],
            depfile_paths: vec![],
            bringup: "demo".into(),
            launch_file: PathBuf::from("/tmp/x.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers,
            node_overrides: Vec::new(), // no [[node_overrides]] needed
            resolved_tiers: None,
        };

        resolve_plan_sched(&mut plan, "posix").expect("resolve_plan_sched should succeed");

        let table = plan
            .resolved_tiers
            .as_ref()
            .expect("resolved_tiers must be set");
        assert!(!table.is_single_tier());
        assert_eq!(table.tiers[0].name, "high");
        assert_eq!(table.tiers[1].name, "low");
        // group_tiers drives the tier assignment directly.
        assert_eq!(
            plan.nodes[0].sched_context,
            Some(0),
            "ctrl (high) must get sched_context=0"
        );
        assert_eq!(
            plan.nodes[1].sched_context,
            Some(1),
            "telem (low) must get sched_context=1"
        );
        // Group members must appear in the tier table.
        assert!(
            table.tiers[0]
                .members
                .contains(&("ctrl".to_string(), "ctrl_grp".to_string())),
            "ctrl/ctrl_grp must be a high-tier member"
        );
        assert!(
            table.tiers[1]
                .members
                .contains(&("telem".to_string(), "telem_grp".to_string())),
            "telem/telem_grp must be a low-tier member"
        );
    }

    /// Phase 269 (W4) — resolve_plan_sched on a plan with no tiers, no overrides,
    /// and no callback groups is a no-op (resolved_tiers stays None).
    #[test]
    fn resolve_plan_sched_no_tiers_is_noop() {
        use std::path::PathBuf;
        let mut plan = Plan {
            board: "native".into(),
            nodes: vec![PlanNode {
                pkg: "talker_pkg".into(),
                exec: "talker".into(),
                name: None,
                namespace: None,
                class_name: None,
                class_header: None,
                lang: Some("c".into()),
                shape: None,
                host: None,
                qos_overrides: Vec::new(),
                params: Vec::new(),
                callback_groups: Vec::new(),
                sched_context: None,
                group_tiers: BTreeMap::new(),
            }],
            depfile_paths: vec![],
            bringup: "demo".into(),
            launch_file: PathBuf::from("/tmp/x.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers: Default::default(),
            node_overrides: Vec::new(),
            resolved_tiers: None,
        };
        resolve_plan_sched(&mut plan, "posix").expect("no-op should succeed");
        assert!(
            plan.resolved_tiers.is_none(),
            "no-tier plan must leave resolved_tiers as None"
        );
        assert!(
            plan.nodes[0].sched_context.is_none(),
            "no-tier plan must leave sched_context as None"
        );
    }
}
