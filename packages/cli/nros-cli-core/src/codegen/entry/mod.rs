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

use eyre::{Result, WrapErr, bail};
use nros_orchestration_ir::{
    CallbackGroupDecl, DEFAULT_TIER, ResolvedTierTable, TierResolveError, resolve_tiers,
};

use crate::orchestration::cargo_metadata_schema::{NodeOverride, TierDef};

pub mod emit_c;
pub mod emit_cpp;
pub mod emit_rust;
#[cfg(test)]
mod golden;
pub mod metadata;
/// phase-432 W2.4 — the shared Rust-entry parity corpus, this side.
#[cfg(test)]
mod parity;

pub mod registered_node;

/// phase-432 W3.2 — the entry pack manifests and the one routing decision.
pub mod pack;
mod render;

/// The entry emitters' target language.
///
/// phase-432 W2.1 — an ALIAS now, not a second declaration. This enum and
/// `orchestration::ComponentLanguage` were the same three variants with no
/// relationship between them, so a language added to one was invisible to the
/// other; issue #1062 is that already shipped. `Lang::parse` moved to
/// `nros_lang::Language::parse` with it, aliases (`c++`, `cxx`) intact.
pub type Lang = nros_lang::Language;

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
    /// Issue 0794 — the image's RFC-0045 baked SESSION rung, folded from the
    /// model's per-node `execution.deploy` entries by [`baked_session`].
    /// `Default` (everything `None`) for a plan whose model declares none,
    /// which is what every hand-built test plan wants.
    pub session: BakedSession,
}

// phase-326 (issue 0364): `Plan::for_host` / `Plan::hosts` / `PlanNode.host`
// are gone. They partitioned a multi-host launch at BAKE time from the
// `<node machine="…">` attribute — ROS 1 roslaunch syntax ROS 2 rejects.
// Multi-host is now a resolve-time launch argument (`host:=<id>` + `if=`
// conditions), so a per-host SystemModel already contains only that host's
// nodes and the bake needs no partition step.

/// Issue #52 / 0303 — one lowered QoS override on a plan node.
///
/// The lowering (parameter key/value strings → `(topic, role, policy, value)`
/// codes) lives in `nros_orchestration_ir::qos_override`, shared with the
/// `nros::main!` proc-macro, and REJECTS what it cannot lower. A plan therefore
/// never carries an override the emitters would silently drop.
pub type QoSOverrideSpec = nros_orchestration_ir::qos_override::LoweredOverride;

/// One Node-pkg invocation in launch order.
///
/// `pkg` is the cargo-style pkg name (sanitised via [`sanitize_pkg`]
/// for symbol-name use). `exec` and `name` come straight from the
/// launch XML; today only `pkg` drives codegen (the per-pkg mangled
/// register symbol is keyed on it), but the `exec` / `name` fields
/// stay on the IR for `<param>` / `<remap>` routing.
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
    /// Phase 211.H (issue #52) — per-topic QoS overrides decomposed from this
    /// node's `qos_overrides.<topic>.<role>.<policy>` launch params. Empty when
    /// none. The typed C++ entry emitter bakes them into a
    /// `node.set_qos_overrides(...)` call before `configure(node)`.
    pub qos_overrides: Vec<QoSOverrideSpec>,
    /// Launch `<param name= value=>` initials (NON-qos params; qos ones go to
    /// `qos_overrides`). Preserved in launch-file order. (#116)
    pub params: Vec<(String, String)>,
    /// Phase 305 W3 (issue 0255) — launch `<remap from= to=/>` rules `(from, to)`,
    /// in launch declaration order (node-level rules first, then group-level —
    /// first match wins at runtime). The typed emitters bake one
    /// `nros_cpp_declare_remap` call per pair so entity registration resolves
    /// `~`/relative names + substitutes matches (executor-side table).
    pub remaps: Vec<(String, String)>,
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

/// How a generated entry drives its executor(s) — the branch BOTH entry packs
/// take, decided once.
///
/// issue 1283 — the C and C++ packs each derived this, and differently. C
/// asked `!tiers.is_empty()`, C++ asked `!is_single_tier()`; `resolve_tiers`
/// synthesises one `default` tier whenever a node declares callback groups
/// without a `[tiers]` table, so for that plan C called `run_tiers` with one
/// tier while C++ called `run_components`. And C had no [`Self::SchedContexts`]
/// arm at all, so a group-split plan fell through to [`Self::Single`] and every
/// group ran at the executor's default scheduling, with nothing reporting it.
/// Issue 1172 was the same shape one layer down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutorShape {
    /// One executor, default scheduling: no tiers, or only the synthesised
    /// `default` one.
    Single,
    /// One executor, with one sched context per tier bound by node name and by
    /// callback group (RFC-0047). The multi-tier plans `run_tiers` cannot
    /// express: a node whose groups span tiers (its per-tier setups construct
    /// whole NODES), or a board with no `run_tiers`.
    SchedContexts,
    /// One executor per tier, through the board's `run_tiers`.
    Tiers,
}

impl Plan {
    /// Whether the resolved table has real tiers, i.e. is anything but the
    /// single synthesised `default` tier (see [`ExecutorShape`], issue 1283).
    pub fn is_multi_tier(&self) -> bool {
        self.resolved_tiers
            .as_ref()
            .is_some_and(|t| !t.is_single_tier())
    }

    /// The executor branch every entry pack takes for this plan. The packs
    /// consume it; none re-derives it (issue 1283).
    ///
    /// A metadata probe is the one caller-side refinement, and it is C++-only:
    /// a probe never takes `run_tiers` (see `emit_cpp`), so it maps
    /// [`ExecutorShape::Tiers`] to [`ExecutorShape::SchedContexts`].
    pub fn executor_shape(&self) -> ExecutorShape {
        let Some(tiers) = self
            .resolved_tiers
            .as_ref()
            .filter(|_| self.is_multi_tier())
        else {
            return ExecutorShape::Single;
        };
        if tiers.has_group_split_node() || !board_has_run_tiers(&self.board) {
            ExecutorShape::SchedContexts
        } else {
            ExecutorShape::Tiers
        }
    }
}

/// Whether the board family has a `run_tiers` runner. ThreadX does not, so a
/// tiered ThreadX plan takes the sched-context path in BOTH packs.
///
/// Issue 1285: this reads the `run_tiers` half of
/// `BoardFamily::c_abi_runners`, the one record of which runners each family
/// exports, and it names no board. Issue 1286 gave ThreadX a
/// `run_components` and no `run_tiers`, and this stayed `false` for it
/// without an edit. Both packs' `run_tiers` sit on those C-ABI symbols.
///
/// An unknown key answers `false` here rather than guessing a family. Every
/// emitter refuses such a key, naming the known ones, before it renders
/// anything, so this answer is never used.
fn board_has_run_tiers(board: &str) -> bool {
    nros_entry_lower::board_family(board)
        .ok()
        .and_then(nros_entry_lower::BoardFamily::c_abi_runners)
        .is_some_and(|r| r.run_tiers.is_some())
}

/// The param-services and lifecycle registrations that close a setup function.
///
/// phase-432 W2.3 — this was a `String` in both views, rendered by Rust from a
/// template and spliced in. It is two facts now, and the pack's own partial is
/// INCLUDED where it belongs rather than interpolated: the last rendered text
/// the entry views carried.
///
/// Both are PROCESS facts, not per-tier ones, which is why the tiered path
/// emits them in tier 0 alone.
#[derive(serde::Serialize)]
pub(crate) struct ServicesView {
    pub param_services: bool,
    /// `None` when the plan declares no lifecycle; otherwise the autostart
    /// code the C ABI takes. "none" | "configure" | anything else (= active).
    pub lifecycle_code: Option<u8>,
}

pub(crate) fn services_view(plan: &Plan) -> ServicesView {
    ServicesView {
        param_services: plan.param_services,
        lifecycle_code: plan.lifecycle.as_deref().map(|a| match a {
            "none" => 0u8,
            "configure" => 1,
            _ => 2,
        }),
    }
}

/// The per-node declarations a pack renders before construction, and the QoS
/// overrides it renders after `create_node`.
///
/// phase-432 W2.3 — these were `emit_declare_remaps` / `emit_declare_params` /
/// `emit_qos_overrides`, string writers shared between the two emitters and
/// carried as pre-rendered `String` fields in both views. They are values now.
/// `exec` is the pack's own executor EXPRESSION, which is a spelling and so
/// belongs to the emitter, not to the lowering.
#[derive(serde::Serialize)]
pub(crate) struct DeclsView {
    /// RAW node name — the pack quotes it.
    pub name: String,
    pub exec: &'static str,
    /// issue 1272 -- the index this node gets on the executor that builds it,
    /// i.e. its position among the nodes ONE setup function creates on ONE
    /// executor (a tier's own, or the process-global one). Its parameter seeds
    /// are declared on this key before the node exists, so it has to be the
    /// index `node_builder` hands out next, not the plan-wide node index.
    pub node: usize,
    pub remaps: Vec<RemapView>,
    pub params: Vec<ParamView>,
}

#[derive(serde::Serialize)]
pub(crate) struct RemapView {
    pub from: String,
    pub to: String,
}

#[derive(serde::Serialize)]
pub(crate) struct ParamView {
    pub key: String,
    pub value: String,
}

/// One QoS override, in codes.
///
/// Issue 0303 — the plan already carries CODES: the lowering (and its
/// rejection of anything unusable) happened in
/// `nros_orchestration_ir::qos_override`, so there is nothing to decode or
/// silently skip at render time.
///
/// Unlike `DeclsView` this gets no shared partial, and the difference is real
/// rather than an omission: C calls a free function on the node's ADDRESS
/// (`nros_cpp_node_set_qos_overrides(&__nros_node_0, …)`) while C++ calls a
/// method on the node, and the element type is spelled `nros_cpp_qos_override_t`
/// in one and `::nros_cpp_qos_override_t` in the other. Same data, two
/// language surfaces — which is exactly what a per-pack template is for.
#[derive(serde::Serialize)]
pub(crate) struct QosRowView {
    pub topic: String,
    pub role: u8,
    pub policy: u8,
    pub value: u32,
}

/// The namespace a plan node's entities register under — the ONE derivation.
///
/// Issue 1443 — this had three authored copies (`tier_group_keys`'s `ns_of`,
/// `sched_view`'s `node_ns`, and `node_binds` inline) and a FOURTH answer
/// spelled as the literal `"/"` in both entry packs' `node_create` call. So a
/// launch node under `/island` was BOUND to its scheduling context by
/// `(name, "/island")` and CREATED at `(name, "/")` three lines away — which
/// meant the bind matched nothing and, on the wire, the node appeared at
/// `/talker` in a C or C++ image and `/island/talker` in the Rust image built
/// from the same launch file.
///
/// `None` and `Some("")` are the SAME answer here: "the model gives this node
/// no namespace", which is the ROOT and never `""`. That is RFC-0045's "unset"
/// versus "configured to nothing", a distinction a `const char*` edge cannot
/// carry, so it is normalised at this end — the same choice
/// `nros_board_native_run_components_named_ns` makes for the session rung and
/// `emit_cpp::plan_node_fqn` already made for the contract-row key.
pub(crate) fn node_namespace(n: &PlanNode) -> &str {
    match n.namespace.as_deref() {
        None | Some("") => "/",
        Some(ns) => ns,
    }
}

/// The declarations for one node, with the pack's executor expression and the
/// node's index on that executor (see [`DeclsView::node`]).
pub(crate) fn decls_view(n: &PlanNode, exec: &'static str, node: usize) -> DeclsView {
    DeclsView {
        name: n.name.as_deref().unwrap_or(&n.exec).to_string(),
        exec,
        node,
        remaps: n
            .remaps
            .iter()
            .map(|(from, to)| RemapView {
                from: from.clone(),
                to: to.clone(),
            })
            .collect(),
        params: n
            .params
            .iter()
            .map(|(key, value)| ParamView {
                key: key.clone(),
                value: value.clone(),
            })
            .collect(),
    }
}

pub(crate) fn qos_views(n: &PlanNode) -> Vec<QosRowView> {
    n.qos_overrides
        .iter()
        .map(|o| QosRowView {
            topic: o.topic.clone(),
            role: o.role,
            policy: o.policy,
            value: o.value,
        })
        .collect()
}

/// One tier, in neutral terms — the row BOTH entry packs render.
///
/// Every field is a VALUE, and the strings are RAW: the pack quotes them
/// through its own escaping filter, and decides for itself that an empty
/// `groups` means `NULL` (C) or `nullptr` (C++) rather than an array symbol.
/// Shared rather than declared twice because a tier spelled two ways is the
/// defect this phase exists to remove — the same reason W1.2's gate compares
/// the two message packs one layer down.
///
/// The ENCODINGS here are ABI facts, not spellings: `core_plus1` is the core
/// index plus one with 0 meaning unpinned, and `preempt_threshold` is -1 when
/// unset. Those belong to the lowering; how they are laid out belongs to the
/// pack.
#[derive(serde::Serialize)]
pub(crate) struct TierView {
    pub index: usize,
    pub name: String,
    /// `(node name, node namespace, group)` per admitted group, FLATTENED to
    /// 3N strings by the pack — `nros_native_tier_spec_t.n_groups` counts
    /// TRIPLES and the array holds three entries each.
    ///
    /// issue 1172 — this was the group id alone, and the two packs derived it
    /// DIFFERENTLY: `emit_c` deduped ACROSS tiers (so a group named by two
    /// tiers emptied the second tier's array, and an empty array is the
    /// WILDCARD — that tier then ran every callback in the image), `emit_cpp`
    /// deduped within each tier. Neither was right, because the filter could
    /// not express the node; with the node in the key there is nothing to
    /// dedup across tiers and the two rules collapse into one.
    pub groups: Vec<(String, String, String)>,
    pub priority: i64,
    pub stack_bytes: u64,
    pub spin_period_us: u64,
    /// 0 = unpinned; otherwise the core index PLUS ONE.
    pub core_plus1: u32,
    /// -1 = unset.
    pub preempt_threshold: i64,
    /// `None` = unset; the pack decides what that is spelled.
    pub class: Option<String>,
    pub period_us: u64,
    pub budget_us: u64,
    pub deadline_us: u64,
    pub deadline_policy: Option<String>,
}

/// The `(node name, namespace, group)` key for every group admitted on each
/// tier — ONE derivation, shared by both entry packs.
///
/// issue 1172 — this used to be written twice and differently. `emit_c` deduped
/// ACROSS tiers, so a group named by two tiers left the SECOND tier's array
/// empty; an empty array is the WILDCARD (`main.h`: "NULL / 0 means wildcard"),
/// so that tier stopped filtering and ran every callback in the image at its
/// own priority. `emit_cpp` deduped WITHIN each tier and kept empty ids. The
/// C comment claimed "a group named by two tiers belongs to the first", which
/// is not what its code did — it disabled the second tier's filter.
///
/// Neither rule was recoverable, because the filter could not express the
/// node. With the node in the key there is nothing to dedup across tiers: two
/// nodes' `ctrl` are two different keys, so the question the old rules
/// disagreed about does not arise.
///
/// The namespace comes from the plan's node, and MUST be the one the entry
/// creates the node with — a filter naming a namespace the node does not have
/// matches nothing, and the tier would register nothing.
pub(crate) fn tier_group_keys(
    tiers: &nros_orchestration_ir::ResolvedTierTable,
    plan: &Plan,
) -> Vec<Vec<(String, String, String)>> {
    let ns_of = |node_name: &str| -> String {
        plan.nodes
            .iter()
            .find(|n| n.name.as_deref().unwrap_or(&n.exec) == node_name)
            .map(node_namespace)
            .unwrap_or("/")
            .to_string()
    };
    tiers
        .tiers
        .iter()
        .map(|tier| {
            let mut keys: Vec<(String, String, String)> = tier
                .members
                .iter()
                // An empty group id names no group and never did; it is not a
                // key, and emitting it would make the tier match an entity
                // whose group is the empty string.
                .filter(|(_, group)| !group.is_empty())
                .map(|(node, group)| (node.clone(), ns_of(node), group.clone()))
                .collect();
            // Only an exact repeat of the same triple is a duplicate.
            keys.sort();
            keys.dedup();
            keys
        })
        .collect()
}

/// Build the shared tier rows.
///
/// `groups_per_tier` stays a PARAMETER, but there is now exactly one thing to
/// pass: [`tier_group_keys`]. It was two authored derivations — issue 1172 —
/// and sharing the ROW first is what made the divergence visible at all.
pub(crate) fn tier_views(
    tiers: &nros_orchestration_ir::ResolvedTierTable,
    groups_per_tier: Vec<Vec<(String, String, String)>>,
) -> Vec<TierView> {
    tiers
        .tiers
        .iter()
        .enumerate()
        .map(|(ti, tier)| TierView {
            index: ti,
            name: tier.name.clone(),
            groups: groups_per_tier[ti].clone(),
            priority: tier.priority,
            stack_bytes: tier.stack_bytes.unwrap_or(0) as u64,
            spin_period_us: tier.spin_period_us.unwrap_or(0),
            core_plus1: tier.core.map(|c| c + 1).unwrap_or(0),
            preempt_threshold: tier.preempt_threshold.unwrap_or(-1),
            class: tier.class.clone(),
            period_us: tier.period_us.unwrap_or(0),
            budget_us: tier.budget_us.unwrap_or(0),
            deadline_us: tier.deadline_us.unwrap_or(0),
            deadline_policy: tier.deadline_policy.clone(),
        })
        .collect()
}

/// The sched-context wiring for [`ExecutorShape::SchedContexts`] — ONE
/// derivation, rendered by both entry packs (issue 1283; the
/// [`tier_group_keys`] pattern from issue 1172). It used to live in `emit_cpp`
/// alone, so the C pack had nothing to render and emitted nothing.
///
/// Every field is a VALUE; strings are RAW and the pack quotes them. `None`
/// means unset, spelled `NULL` in C and `nullptr` in C++.
#[derive(serde::Serialize)]
pub(crate) struct SchedView {
    pub n: usize,
    pub contexts: Vec<SchedContextView>,
    pub node_binds: Vec<NodeBindView>,
    pub group_binds: Vec<GroupBindView>,
}

/// One tier's RTOS-agnostic policy, as `nros_cpp_create_sched_context_from_policy`
/// takes it. RAW tier fields only: the call lowers them through
/// `SchedContext::from_tier_policy`, the SAME lowering the Rust runtime's
/// `apply_tier_sched_policy` uses (RFC-0052), so the mapping cannot drift
/// between languages.
#[derive(serde::Serialize)]
pub(crate) struct SchedContextView {
    pub index: usize,
    pub class: Option<String>,
    pub period_us: u64,
    pub budget_us: u64,
    pub deadline_us: u64,
    pub deadline_policy: Option<String>,
    pub os_pri: u8,
}

#[derive(serde::Serialize)]
pub(crate) struct NodeBindView {
    pub name: String,
    pub namespace: String,
    pub sched_context: u8,
}

#[derive(serde::Serialize)]
pub(crate) struct GroupBindView {
    pub name: String,
    pub namespace: String,
    pub group: String,
    pub tier_index: usize,
}

/// Build the sched-context wiring for a plan whose [`Plan::executor_shape`]
/// is [`ExecutorShape::SchedContexts`].
pub(crate) fn sched_view(tiers: &ResolvedTierTable, plan: &Plan) -> SchedView {
    let contexts = tiers
        .tiers
        .iter()
        .enumerate()
        .map(|(ti, tier)| SchedContextView {
            index: ti,
            class: tier.class.clone(),
            period_us: tier.period_us.unwrap_or(0),
            budget_us: tier.budget_us.unwrap_or(0),
            deadline_us: tier.deadline_us.unwrap_or(0),
            deadline_policy: tier.deadline_policy.clone(),
            os_pri: tier.priority.clamp(0, 255) as u8,
        })
        .collect();

    let node_ns = |name: &str| -> String {
        plan.nodes
            .iter()
            .find(|n| n.name.as_deref().unwrap_or(&n.exec) == name)
            .map(node_namespace)
            .unwrap_or("/")
            .to_string()
    };

    let node_binds = plan
        .nodes
        .iter()
        .filter_map(|n| {
            n.sched_context.map(|sc| NodeBindView {
                name: n.name.as_deref().unwrap_or(&n.exec).to_string(),
                namespace: node_namespace(n).to_string(),
                sched_context: sc,
            })
        })
        .collect();

    let group_binds = tiers
        .tiers
        .iter()
        .enumerate()
        .flat_map(|(ti, tier)| {
            tier.members
                .iter()
                .map(move |(node_name, group)| (ti, node_name.clone(), group.clone()))
        })
        .map(|(ti, node_name, group)| GroupBindView {
            namespace: node_ns(&node_name),
            name: node_name,
            group,
            tier_index: ti,
        })
        .collect();

    SchedView {
        n: tiers.tiers.len(),
        contexts,
        node_binds,
        group_binds,
    }
}

/// Issue 0794 — the image-level half of the RFC-0045 baked rung: the SESSION
/// facts the `.nros_boot_config` blob carries beside the node's identity.
///
/// The model states these PER NODE (`execution.deploy.<fqn>.{domain, locator,
/// rmw}`, documented there as "RFC-0045 baked rung on embedded"), because a
/// deploy entry is per node. The blob has ONE of each, because an image opens
/// one session. [`baked_session`] is the fold between the two, and it is the
/// only place that decision is taken.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BakedSession {
    /// `execution.deploy.<fqn>.domain`, when every deployed node agrees.
    ///
    /// `Some(0)` is a real answer and NOT the same as `None`: domain 0 is a
    /// legal ROS domain, which is the whole reason the blob carries presence
    /// BITS rather than reading a zero field as unset.
    pub domain: Option<u8>,
    /// `execution.deploy.<fqn>.locator`, when every deployed node agrees.
    /// An empty string is normalised to `None` — issue 0330's "absent, let the
    /// backend discover" is the established meaning of an empty locator, so
    /// baking `""` WITH the bit set would assert a configured-empty endpoint.
    pub locator: Option<String>,
    /// `execution.deploy.<fqn>.rmw`, when every deployed node agrees. Empty is
    /// normalised to `None` for the same reason `nros_support_init` does it
    /// (issue 1050 defect (3)): an empty selector is "unset", never a backend
    /// named `""`.
    pub rmw: Option<String>,
    /// Field names at least one node DECLARED but the image could not agree
    /// on, in `domain`/`locator`/`rmw` order.
    ///
    /// The bit is left CLEAR for these — the blob cannot represent "node A on
    /// domain 5, node B on the default", and baking either one would put a
    /// fact in the image that no node asked for. That is not a silent drop:
    /// the generated blob carries a comment naming each field here, which is
    /// where a reader of the emitted TU is already looking.
    pub conflicts: Vec<String>,
}

/// Fold per-node deploy facts into one image-level answer.
///
/// Unanimity, and nothing weaker: every node must declare the SAME value.
/// Any node silent, or any two disagreeing, yields `None` — a partial
/// declaration is a placement the blob cannot express, not a majority vote.
///
/// Returns `(value, declared_by_someone)`; the caller reports the second as a
/// conflict when the first is `None`.
fn unanimous<T: Clone + PartialEq>(vals: &[Option<T>]) -> (Option<T>, bool) {
    let declared = vals.iter().any(Option::is_some);
    let mut it = vals.iter();
    let first = match it.next() {
        Some(v) => v.clone(),
        None => return (None, false),
    };
    if first.is_some() && it.all(|v| *v == first) {
        (first, declared)
    } else {
        (None, declared)
    }
}

/// Build a [`BakedSession`] from the deployed nodes' `(domain, locator, rmw)`.
///
/// The empty-string normalisation happens HERE, before the fold, so that a
/// `Plan` never carries `Some("")` for a field whose bit would then be set on
/// a value that means "unset" to every reader.
fn baked_session(per_node: &[(Option<u8>, Option<String>, Option<String>)]) -> BakedSession {
    fn nonempty(s: &Option<String>) -> Option<String> {
        s.as_deref().filter(|v| !v.is_empty()).map(str::to_string)
    }
    let domains: Vec<Option<u8>> = per_node.iter().map(|(d, _, _)| *d).collect();
    let locators: Vec<Option<String>> = per_node.iter().map(|(_, l, _)| nonempty(l)).collect();
    let rmws: Vec<Option<String>> = per_node.iter().map(|(_, _, r)| nonempty(r)).collect();

    let (domain, domain_declared) = unanimous(&domains);
    let (locator, locator_declared) = unanimous(&locators);
    let (rmw, rmw_declared) = unanimous(&rmws);

    let mut conflicts = Vec::new();
    if domain.is_none() && domain_declared {
        conflicts.push("domain".to_string());
    }
    if locator.is_none() && locator_declared {
        conflicts.push("locator".to_string());
    }
    if rmw.is_none() && rmw_declared {
        conflicts.push("rmw".to_string());
    }
    BakedSession {
        domain,
        locator,
        rmw,
        conflicts,
    }
}

/// Phase 266 (W5b/W6) — the `NROS_BOOT_CONFIG` blob, as its template sees it.
///
/// phase-432 W2.3 — this used to be `emit_boot_config_static`, a string writer
/// both entry emitters called and both carried as a pre-rendered `String` in
/// their view. It is now a fact set rendered by ONE shared partial
/// (`boot_config.jinja`), which is what "the IR carries no rendered text"
/// means for this field.
///
/// The five facts split into two kinds, and the kind decides what a multi-node
/// image gets:
///
/// * **Identity** — `node_name`, `namespace`. Per NODE, so a multi-node plan
///   (or a single node with no resolvable name) leaves both `None`: the flags
///   come out clear, `nros_boot_config_node_name` returns NULL, and the runner
///   falls back to the unified `"node"` default. There is no such thing as
///   "the" name of an image that runs three nodes.
/// * **Session** — `domain`, `locator`, `rmw`. Per IMAGE, so they survive a
///   multi-node plan whenever every deployed node agrees on them
///   ([`BakedSession`]). This is the "emit what is COMMON" half: a two-node
///   image whose nodes both deploy to domain 7 bakes domain 7; one whose nodes
///   disagree bakes neither and says so.
///
/// issue 0794 — the producer used to set `NROS_BOOT_SET_NODE_NAME` and nothing
/// else, while the blob defines five fields and the reader
/// (`nros-node/src/executor/types.rs`) branches on all five. The namespace half
/// was fixed 2026-08-25; the session half is this. Measured before the fix, on
/// a bringup declaring all three: `.set_flags = NROS_BOOT_SET_NODE_NAME |
/// NROS_BOOT_SET_NAMESPACE`, `.domain_id = 0`, `.locator = ""`, `.rmw = ""`.
#[derive(serde::Serialize)]
pub(crate) struct BootConfigView {
    /// RAW. The pack quotes it.
    pub node_name: Option<String>,
    pub namespace: Option<String>,
    /// RAW `uint32_t` — the blob's field is `uint32_t`, the model's is `u8`.
    pub domain: Option<u32>,
    /// RAW. The pack quotes it.
    pub locator: Option<String>,
    /// RAW. The pack quotes it.
    pub rmw: Option<String>,
    /// [`BakedSession::conflicts`], rendered as a comment above the blob.
    pub conflicts: Vec<String>,
}

/// Resolve the blob's five facts, refusing a value the C field cannot hold.
///
/// # Errors
///
/// Returns `Err` when a resolved string exceeds its fixed C buffer minus the
/// NUL: `node_name` and `namespace_` are `char [64]` (63 bytes), `locator` is
/// `char [96]` (95) and `rmw` is `char [32]` (31). The caller gets a clear
/// diagnostic instead of a confusing C-compiler array-initialiser error. This
/// is a correctness check, so it stays in compiled Rust rather than moving into
/// the template with the layout.
///
/// The domain is NOT range-checked here. `DOMAIN_ID_MAX` lives in `nros-node`,
/// which this crate does not depend on, and mirroring the constant would be a
/// second authored copy of a cap the resolver already enforces at boot
/// (`BootConfigError::DomainIdRange`). A too-large baked domain fails loud
/// there rather than silently becoming domain 0.
pub(crate) fn boot_config_view(plan: &Plan) -> Result<BootConfigView, String> {
    fn fits(what: &str, raw: &str, field: &str, cap: usize) -> Result<(), String> {
        if raw.len() > cap {
            return Err(format!(
                "node {what} '{raw}' is {} bytes; the .nros_boot_config {field} field \
                 holds at most {cap} bytes + NUL",
                raw.len(),
            ));
        }
        Ok(())
    }

    // Session facts are image-level: they are resolved the same way whether the
    // plan holds one node or ten. Identity is resolved below, and only for one.
    let s = &plan.session;
    if let Some(loc) = s.locator.as_deref() {
        fits("locator", loc, "locator", 95)?;
    }
    if let Some(rmw) = s.rmw.as_deref() {
        fits("rmw", rmw, "rmw", 31)?;
    }
    let mut view = BootConfigView {
        node_name: None,
        namespace: None,
        domain: s.domain.map(u32::from),
        locator: s.locator.clone(),
        rmw: s.rmw.clone(),
        conflicts: s.conflicts.clone(),
    };

    if plan.nodes.len() != 1 {
        return Ok(view);
    }
    let n = &plan.nodes[0];
    let raw = n.name.as_deref().unwrap_or(&n.exec);
    fits("name", raw, "node_name", 63)?;
    if let Some(ns) = n.namespace.as_deref() {
        fits("namespace", ns, "namespace_", 63)?;
        view.namespace = Some(ns.to_string());
    }
    view.node_name = Some(raw.to_string());
    Ok(view)
}

/// Sanitise a pkg name into a valid identifier (`-` → `_`).
///
/// phase-432 W2.4 — DELEGATES to [`nros_entry_lower::sanitize_pkg`]. This body
/// and `main_macro.rs::pkg_to_crate_ident` were character-for-character
/// identical, which made "which identifier does this package become" a
/// question with two authored answers and no test that they agreed. The
/// cmake fn `nano_ros_node_register()` applies the same rule from its own
/// side; it is not Rust and cannot call this one.
pub fn sanitize_pkg(pkg: &str) -> String {
    nros_entry_lower::sanitize_pkg(pkg)
}

/// Issue #52 — decompose `qos_overrides.<topic>.<role>.<policy>` parameters
/// into the baked override table, sorted for deterministic emission.
///
/// Issue 0303 — this REJECTS an unusable override (unknown role or policy,
/// unparseable value) instead of filtering it away. Silence is the wrong
/// failure mode for QoS: the image would run different delivery semantics than
/// the model declares, invisibly.
fn qos_overrides_from_params(params: &[(String, String)]) -> Result<Vec<QoSOverrideSpec>> {
    nros_orchestration_ir::qos_override::lower_all(
        params.iter().map(|(k, v)| (k.as_str(), v.as_str())),
    )
    .map_err(|e| eyre::eyre!("{e}"))
}

/// The complement of [`qos_overrides_from_params`]: everything that is NOT a
/// QoS override, which is what gets baked as declared parameters.
fn non_qos_params(params: &[(String, String)]) -> Vec<(String, String)> {
    params
        .iter()
        .filter(|(name, _)| !nros_orchestration_ir::qos_override::is_qos_override(name))
        .cloned()
        .collect()
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
    use ros_launch_manifest_model::Target;

    let model = model_ingest::load_model(model_path)?;
    // phase-454 W7 (RFC-0100 D8) -- before anything is baked, the contract and
    // the `qos_overrides.*` parameters must state the same QoS.
    //
    // Checked on the BAKE road and not only where the contract is SIZED from,
    // because these are the two producers and only this one is guaranteed to
    // see an override: an image can bake `qos_overrides./t.subscription.depth =
    // 64` without ever running `nros ws entity-inventory --model`, and that is
    // issue 1190 exactly. The check is a property of the model, so it runs
    // before the board slice -- a divergence is not something a board filter
    // can make true.
    nros_orchestration_ir::qos_agreement::check_model(&model)
        .map_err(|e| eyre::eyre!("model `{}`: {e}", model_path.display()))?;
    // phase-462 W2 -- the same ruling for `on_violation`: the contract's word
    // and the tier table's `deadline_policy` are two statements about one
    // fact. The map is applied to the tier defs below; the error is raised
    // here, before anything is baked, for the reason the QoS check above
    // gives.
    let contract_deadline_policies =
        nros_orchestration_ir::violation_agreement::contract_deadline_policies(&model)
            .map_err(|e| eyre::eyre!("model `{}`: {e}", model_path.display()))?;
    // ... and the rows the lowering produces, whose own refusals are about a
    // reaction the image could not run (an unknown reaction path, an
    // `on_violation` with no `max_age_ms` to detect it by).
    model_ingest::violation_rows(&model)
        .map_err(|e| eyre::eyre!("model `{}`: {e}", model_path.display()))?;
    let board = board.unwrap_or_else(|| "native".to_string());
    // Issue 1285 — only a key the entry table knows HAS a tier sub-table.
    //
    // This function is shared: `nros codegen entry` (C/C++, where the key must
    // name a family) and `nros build`'s Rust entry generation, where the key is
    // any Rust board — `esp32-c3-baremetal`, an out-of-tree board — and only
    // the node list is read. So an unknown key is NOT refused here; its tiers
    // are resolved from their platform-neutral head (`TierDef::platform("")`
    // selects no sub-table) rather than from `posix`'s, which the old substring
    // fallback silently picked. Every consumer that needs the family asks
    // `board_family`/`board_to_rtos` itself and refuses the key there.
    //
    // The lenient rule is one function, `tier_rtos_key_for`, shared with the
    // `nros::main!` proc-macro, which answers the same Rust keys (issue 1285
    // follow-up). It used to be spelled `unwrap_or("")` here and `"posix"`
    // there.
    let target_rtos = nros_entry_lower::tier_rtos_key_for(&board);

    // phase-315 / issue 0288 — does the model place ANYTHING on this board?
    //
    // `execution.deploy` is a PARTITION (node -> one target). That is right for
    // machines and wrong for board build-variants, where every board runs every
    // node and the map cannot say so. A board the map never mentions is
    // therefore not "a board with nothing on it" — it is a board the model has
    // no opinion about, which is exactly what the `(None, _) => true` arm below
    // already says per-NODE, lifted to the board level.
    //
    // This is the SECOND copy of this rule: `nros-macros`'s `main_macro.rs` has
    // the same filter for the Rust path and was fixed first. Fixing one and not
    // the other is why `examples/workspaces/c`'s freertos entry still failed
    // with `places no nodes on board mps2-an385-freertos` after the Rust side
    // was green. Two entry emitters, one rule — see issue 0358 for the class.
    let board_mentioned = model.execution.deploy.values().any(|d| match &d.target {
        Some(Target::Linux) => matches!(board.as_str(), "native" | "posix"),
        Some(Target::Mcu { board: b }) => {
            b == &board
                || matches!(
                    d.extra.get("kind"),
                    Some(ros_launch_manifest_model::ExtraValue::Str(k)) if k == &board
                )
        }
        None => false,
    });

    let keep = |fqn: &str| -> bool {
        let Some(dep) = model.execution.deploy.get(fqn) else {
            return model.execution.deploy.is_empty();
        };
        if !board_mentioned {
            return true;
        }
        match (&dep.target, board.as_str()) {
            // Board-agnostic (multi-board system, issue 0356): no board
            // named — this entry's board decides.
            (None, _) => true,
            (Some(Target::Linux), "native" | "posix") => true,
            // Exact board key, or the deploy's platform kind (the
            // integrator's `[deploy.<t>] kind = "zephyr"`, carried in
            // `extra.kind`) — the entry codegen key is the board FAMILY
            // ("zephyr") while deploys name the concrete board
            // ("fvp-aemv8r-smp"), so both spellings must slice.
            (Some(Target::Mcu { board: b }), key) => {
                b == key
                    || matches!(
                        dep.extra.get("kind"),
                        Some(ros_launch_manifest_model::ExtraValue::Str(k)) if k == key
                    )
            }
            _ => false,
        }
    };

    let mut nodes: Vec<PlanNode> = Vec::new();
    // Issue 0794 — the RFC-0045 baked SESSION rung, collected per deployed node
    // and folded once below. `execution.deploy.<fqn>.{domain, locator, rmw}` is
    // the model's own name for this ("RFC-0045 baked rung on embedded"), and it
    // was read here for the board SLICE and for nothing else, so a launch that
    // declared a domain or a locator produced an image whose blob said neither.
    let mut deploy_facts: Vec<(Option<u8>, Option<String>, Option<String>)> = Vec::new();
    for (fqn, inst) in &model.structure.nodes {
        if !keep(fqn) {
            continue;
        }
        deploy_facts.push(match model.execution.deploy.get(fqn) {
            Some(d) => (d.domain, d.locator.clone(), d.rmw.clone()),
            None => (None, None, None),
        });
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
        // #276 (model path) — project `params_files` YAML under the inline
        // `<param>` values. The launch path has its own projection
        // (`resolve_node_params`), but every live bake now goes through the
        // MODEL, so the projection has to exist here too.
        let resolved: Vec<(String, String)> = inst
            .resolved_params(fqn)
            .iter()
            .map(|(k, v)| (k.clone(), v.to_bake_string()))
            .collect();
        let qos_overrides =
            qos_overrides_from_params(&resolved).wrap_err_with(|| format!("node `{fqn}`"))?;
        let params = non_qos_params(&resolved);
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
            qos_overrides,
            params,
            remaps: inst
                .remaps
                .iter()
                .map(|r| (r.from.clone(), r.to.clone()))
                .collect(),
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

    // phase-459 W2 (issue 1426) - an EMPTY table here is not the end of the
    // question any more. A cmake image authors no `[tiers.*]` and states its
    // callback groups in the registration instead; the entry derives its own
    // rate-monotonic table from the model's contract layer in
    // [`derive_entry_tiers`], which runs after [`metadata::enrich_plan`] has
    // put those groups on the plan. It cannot run here: the groups are not on
    // the plan yet, and the SystemModel carries none.
    let mut tiers: BTreeMap<String, TierDef> = model
        .execution
        .tiers
        .iter()
        .map(|(name, t)| {
            (
                name.clone(),
                crate::orchestration::model_ingest::tier_from_model(t, target_rtos),
            )
        })
        .collect();
    // phase-462 W2 -- the contract is the SOURCE of `deadline_policy`, and
    // this is where it becomes one. From here the word rides the ordinary
    // tier road: `resolve_tiers` -> `ResolvedTier::deadline_policy` ->
    // `tier_views`/`sched_view` -> the baked entry ->
    // `SchedContext::from_tier_policy` -> `DeadlineAction`. Agreement with an
    // authored string was already checked above, so an insert here either
    // supplies what the tier table omitted or restates what it agreed to.
    //
    // A tier the contract says nothing about keeps whatever it authored: the
    // contract abstaining is not the contract saying `ignore`.
    //
    // Declared tiers only. A node with no callback group has no tier to land
    // on (`derive.rs:93-100` notes it and moves on), so on a groupless image
    // this map is empty by construction -- phase-459 W2 is what gives such a
    // node a SchedContext, and until it lands W2's deadline half reaches
    // exactly the images that declare tiers.
    for (tier, action) in &contract_deadline_policies {
        if let Some(def) = tiers.get_mut(tier) {
            def.deadline_policy = Some(action.clone());
        }
    }

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
        session: baked_session(&deploy_facts),
    })
}

/// Phase 269 (W4): the RTOS key [`resolve_tiers`] recognises, derived from a
/// board deploy key. It is the key `rtos_spec` in `nros-orchestration-ir`
/// reads (`"posix"`, `"freertos"`, …).
///
/// Issue 1285: read from `nros_entry_lower::BOARD_KEYS`, the one key →
/// family table. This used to be a SUBSTRING match with a `posix` fallback,
/// so `s32z270`, `an536` (FreeRTOS) and `armfvp`, `fvp-aemv8r-smp` (Zephyr)
/// all silently got the POSIX tier sub-table. An unknown key is now an error
/// naming the known ones.
pub fn board_to_rtos(board: &str) -> Result<&'static str, nros_entry_lower::UnknownBoard> {
    nros_entry_lower::board_family(board).map(nros_entry_lower::BoardFamily::tier_rtos_key)
}

/// phase-459 W2 (issue 1426) - THE ENTRY DERIVES ITS OWN SCHEDULE.
///
/// A cmake image states its callback groups once, in the registration, and
/// authors no `[tiers.*]`. `codegen-system` derives a rate-monotonic table
/// from that (phase-459 W1), but the entry is generated by
/// `nano_ros_add_executable` BEFORE and independently of the Zephyr module's
/// bake, so it cannot read what the bake derived: it would be a file
/// dependency between two generators, i.e. a second, ordered reading of one
/// fact. The entry therefore runs the SAME
/// [`nros_orchestration_ir::derive::derive_tiers_from_contracts`] over the
/// SAME model with the same target RTOS key, so `codegen entry --board zephyr`
/// and `codegen-system --for-entry zephyr_entry` cannot disagree.
///
/// Engages only when every one of these holds:
///
/// * the model authored NO tiers and no `[[node_overrides]]` - declared tiers
///   always win, the same rule `codegen_system.rs` applies;
/// * some node carries callback groups (from cmake metadata, via
///   [`metadata::enrich_plan`]) - a node with none has nothing for the gating
///   executor to bind, and inventing a group would place code on a tier
///   nobody asked for;
/// * the plan came from a resolved SystemModel, which is the only input that
///   carries a contract layer to derive FROM.
///
/// Returns the number of derived tiers (0 = nothing derived; the caller then
/// resolves exactly as before).
///
/// # Why here and not in [`plan_from_model`]
///
/// The phase doc places this in `plan_from_model`. It cannot go there: the
/// groups arrive on the plan from `nros-metadata.json` through
/// [`metadata::enrich_plan`], which runs AFTER the plan is built, and the
/// SystemModel schema carries no callback groups at all (rlm v0.1.37 has no
/// such field - that is issue 1426's "groups are a property of the code").
/// Deriving in `plan_from_model` would mean reading the cmake metadata a
/// second time, from a workspace root that function is not given, which is
/// the duplicate-reader failure W1 removed one layer down. The model is
/// re-read from `plan.launch_file` here instead: one extra parse of a file
/// this bake has already validated, against two readers of one fact.
fn derive_entry_tiers(plan: &mut Plan, target_rtos: &str, has_groups: bool) -> Result<usize> {
    if !plan.tiers.is_empty() || !plan.node_overrides.is_empty() || !has_groups {
        return Ok(0);
    }
    // `plan_from_model` records the model path as the plan's `launch_file` and
    // marks the bringup `system-model`; every other producer (the cmake
    // registered-node road, the hand-built test plans) has no contract layer
    // and nothing to derive from.
    if plan.bringup != "system-model" || !plan.launch_file.is_file() {
        return Ok(0);
    }
    let model = crate::orchestration::model_ingest::load_model(&plan.launch_file)?;
    if !model.execution.tiers.is_empty() {
        // The plan's tier table is built from this one, so this is unreachable
        // by construction; kept as the guard the rule is stated in.
        return Ok(0);
    }

    // The same input shape `codegen-system` hands the derivation: bare node
    // name -> its declared groups, each bound to DEFAULT_TIER, because the
    // keyword states WHICH groups the code has and never where they run.
    let mut callback_groups: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::new();
    for n in &plan.nodes {
        if n.callback_groups.is_empty() {
            continue;
        }
        let name = n.name.as_deref().unwrap_or(n.exec.as_str()).to_string();
        callback_groups.insert(
            name,
            n.callback_groups
                .iter()
                .map(|g| CallbackGroupDecl {
                    id: g.clone(),
                    r#type: "MutuallyExclusive".to_string(),
                    tier: DEFAULT_TIER.to_string(),
                })
                .collect(),
        );
    }

    let derived = nros_orchestration_ir::derive::derive_tiers_from_contracts(
        &model,
        target_rtos,
        &callback_groups,
    );
    if derived.tiers.is_empty() {
        return Ok(0);
    }
    // Fail-loud, exactly as the bake does (`derive_execution_from_contracts`):
    // a weakened guarantee and a node left on the default tier are both things
    // the person running this build has to be able to see.
    for d in &derived.degradations {
        eprintln!(
            "codegen entry: derived-schedule degradation - {} [{}]: {}",
            d.node, d.dim, d.reason
        );
    }
    for name in &derived.groupless_notes {
        eprintln!(
            "codegen entry: derived-schedule note - node '{name}' declares no \
             callback groups; it stays on the default tier"
        );
    }
    let n = derived.tiers.len();
    plan.tiers = derived.tiers;
    plan.node_overrides = derived.overrides;
    Ok(n)
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
///
/// phase-459 W2 (issue 1426) - when the model authored NO tiers and the
/// nodes DO carry groups, this is also where the entry derives its own
/// schedule ([`derive_entry_tiers`]), so a cmake image runs `run_tiers` over
/// the derived table instead of `run_components`.
pub fn resolve_plan_sched(plan: &mut Plan, target_rtos: &str) -> Result<()> {
    let has_groups = plan
        .nodes
        .iter()
        .any(|n| !n.callback_groups.is_empty() || !n.group_tiers.is_empty());
    if plan.tiers.is_empty() && plan.node_overrides.is_empty() && !has_groups {
        return Ok(());
    }
    derive_entry_tiers(plan, target_rtos, has_groups)?;

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
            .wrap_err_with(|| format!("create depfile parent `{}`", parent.display()))?;
    }
    std::fs::write(depfile, out)
        .wrap_err_with(|| format!("write depfile `{}`", depfile.display()))?;
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
            qos_overrides: Vec::new(),
            params: Vec::new(),
            remaps: Vec::new(),
            callback_groups: Vec::new(),
            sched_context: None,
            group_tiers: BTreeMap::new(),
        };
        assert_eq!(n.cmake_link_target(), "talker_pkg_talker_component");
    }

    /// Issue 0276 + phase-54 — a model's `params_files` project into
    /// `PlanNode.params` at codegen time, and `qos_overrides.*` split out into
    /// typed [`QoSOverrideSpec`]s instead of being baked as parameters.
    ///
    /// The resolution algorithm itself lives in the shared `model` crate; this
    /// asserts the WIRING, not a second copy. The param file writes the node
    /// block ABOVE the wildcard on purpose: section precedence is by
    /// specificity, not file order, so this fails if that ordering is lost.
    #[test]
    fn model_params_project_with_specificity_and_qos_split() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let model = tmp.path().join("system_model.yaml");
        std::fs::write(
            &model,
            r#"
meta:
  version: 1
structure:
  scopes:
    /: {}
  nodes:
    /talker:
      scope: /
      pkg: talker_pkg
      exec: talker
      node_name: talker
      params:
        rate: "50"
        qos_overrides./chatter.publisher.reliability: best_effort
      params_files:
        - "talker:\n  ros__parameters:\n    use_sim_time: true\n    limits:\n      max_accel: 1.5\n/**:\n  ros__parameters:\n    use_sim_time: false\n    rate: 10\nother_node:\n  ros__parameters:\n    rate: 999\n"
execution: {}
contracts: {}
"#,
        )
        .unwrap();

        let plan = plan_from_model(&model, Some("native".into())).expect("plan from model");
        let node = &plan.nodes[0];
        let params: BTreeMap<_, _> = node.params.iter().cloned().collect();

        // The node block sets it true and the wildcard false, with the node
        // block written FIRST — specificity decides, not file order.
        assert_eq!(params.get("use_sim_time").map(String::as_str), Some("true"));
        // Nested map flattened to a dotted key; a float keeps its ".0".
        assert_eq!(
            params.get("limits.max_accel").map(String::as_str),
            Some("1.5")
        );
        // Inline model params outrank the file.
        assert_eq!(params.get("rate").map(String::as_str), Some("50"));
        // A section naming a different node contributes nothing.
        assert!(!params.values().any(|v| v == "999"));
        // QoS travels typed, never as a declared parameter.
        assert!(
            !params.keys().any(|k| k.starts_with("qos_overrides.")),
            "qos override leaked into params: {params:?}"
        );
        // Lowered to codes at plan time (issue 0303): publisher(0),
        // reliability(0), best_effort(0).
        assert_eq!(node.qos_overrides.len(), 1);
        assert_eq!(node.qos_overrides[0].topic, "/chatter");
        assert_eq!(
            node.qos_overrides[0].role,
            nros_orchestration_ir::qos_override::role::PUBLISHER
        );
        assert_eq!(
            node.qos_overrides[0].policy,
            nros_orchestration_ir::qos_override::policy::RELIABILITY
        );
        assert_eq!(node.qos_overrides[0].value, 0);
    }

    /// #236 / issue 0356 — a board-agnostic deploy (`target: None`, a
    /// multi-board system's placement) keeps the node on every board's
    /// slice. An explicit `target: linux` still rejects non-native boards.
    #[test]
    fn model_unplaced_target_is_board_agnostic() {
        use ros_launch_manifest_model::{Deploy, Target};
        let unplaced = Deploy::default();
        assert_eq!(unplaced.target, None);
        let placed_linux = Deploy {
            target: Some(Target::Linux),
            ..Default::default()
        };

        // Mirror the `keep` board match used by plan_from_model + the macro.
        let board_ok = |dep: &Deploy, key: &str| -> bool {
            match (&dep.target, key) {
                (None, _) => true,
                (Some(Target::Linux), "native" | "posix") => true,
                (Some(Target::Mcu { board: b }), key) => b == key,
                _ => false,
            }
        };
        for board in ["native", "posix", "zephyr", "freertos"] {
            assert!(board_ok(&unplaced, board), "unplaced must keep on {board}");
        }
        assert!(board_ok(&placed_linux, "native"));
        assert!(
            !board_ok(&placed_linux, "zephyr"),
            "explicit linux placement must still reject a zephyr board"
        );
    }

    /// Phase 211.H (issue #52) — `qos_overrides.<topic>.<role>.<policy>` params
    /// decompose into sorted `QoSOverrideSpec`s; non-matching params are ignored.
    #[test]
    fn qos_overrides_decompose_from_params() {
        let params = vec![
            (
                "qos_overrides./chatter.publisher.reliability".to_string(),
                "best_effort".to_string(),
            ),
            ("use_sim_time".to_string(), "true".to_string()),
            (
                "qos_overrides./chatter.subscription.durability".to_string(),
                "transient_local".to_string(),
            ),
        ];
        use nros_orchestration_ir::qos_override::{policy, role};
        let got = qos_overrides_from_params(&params).expect("lower");
        assert_eq!(got.len(), 2);
        // sorted (topic, role, policy): publisher before subscription.
        assert_eq!(got[0].role, role::PUBLISHER);
        assert_eq!(got[0].policy, policy::RELIABILITY);
        assert_eq!(got[0].topic, "/chatter");
        assert_eq!(got[0].value, 0); // best_effort
        assert_eq!(got[1].role, role::SUBSCRIPTION);
        assert_eq!(got[1].policy, policy::DURABILITY);
        assert_eq!(got[1].value, 1); // transient_local
    }

    /// Issue 0303 — an override the bake cannot lower FAILS the codegen; it is
    /// not filtered away. Silence would ship different delivery semantics than
    /// the model declares, with nothing to read.
    #[test]
    fn an_unusable_qos_override_fails_codegen() {
        let params = vec![(
            // `pub` instead of `publisher` — the typo that used to vanish.
            "qos_overrides./chatter.pub.reliability".to_string(),
            "reliable".to_string(),
        )];
        let err = qos_overrides_from_params(&params).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("qos_overrides./chatter.pub.reliability"),
            "{msg}"
        );
        assert!(msg.contains("publisher"), "{msg}");
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
                qos_overrides: Vec::new(),
                params: Vec::new(),
                remaps: Vec::new(),
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
            session: Default::default(),
        }
    }

    /// The boot-config blob as a compiler would see it.
    ///
    /// phase-432 W2.3 — these tests assert on emitted TEXT, and the text now
    /// comes from the shared partial rather than from a Rust string writer. So
    /// the helper renders the partial: the assertions keep their meaning AND
    /// gain coverage of the template, where before they covered only the
    /// writer.
    fn boot_config_text(plan: &Plan) -> Result<String, String> {
        #[derive(serde::Serialize)]
        struct Ctx {
            boot_config: BootConfigView,
        }
        super::render::render(
            "boot_config.jinja",
            &Ctx {
                boot_config: boot_config_view(plan)?,
            },
        )
    }

    /// Phase 266 (W6) — a node name of exactly 63 bytes must succeed (fits in
    /// `char node_name[64]` with one byte left for the NUL terminator).
    #[test]
    fn boot_config_node_name_63_bytes_ok() {
        let name = "a".repeat(63);
        assert_eq!(name.len(), 63);
        let plan = single_node_plan(&name);
        let out = boot_config_text(&plan).expect("63-byte name should be accepted");
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
        let err = boot_config_text(&plan).expect_err("64-byte name must be rejected");
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
        let params = vec![
            ("p".to_string(), "v".to_string()),
            (
                "qos_overrides./chatter.publisher.reliability".to_string(),
                "best_effort".to_string(),
            ),
            ("count".to_string(), "42".to_string()),
        ];
        let non_qos = non_qos_params(&params);
        assert_eq!(non_qos.len(), 2);
        assert_eq!(non_qos[0], ("p".into(), "v".into()));
        assert_eq!(non_qos[1], ("count".into(), "42".into()));
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
                time_slice_us: None,
                sched_class: None,
                core: None,
                deadline_us: None,
                budget_us: None,
                period_us: None,
            }),
            ..Default::default()
        };
        let low_tier = TierDef {
            spin_period_us: Some(100_000),
            posix: Some(TierRtosSpec {
                priority: 10,
                stack_bytes: None,
                preempt_threshold: None,
                time_slice_us: None,
                sched_class: None,
                core: None,
                deadline_us: None,
                budget_us: None,
                period_us: None,
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
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    remaps: Vec::new(),
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
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    remaps: Vec::new(),
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
            session: Default::default(),
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
                    time_slice_us: None,
                    sched_class: None,
                    core: None,
                    deadline_us: None,
                    budget_us: None,
                    period_us: None,
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
                    time_slice_us: None,
                    sched_class: None,
                    core: None,
                    deadline_us: None,
                    budget_us: None,
                    period_us: None,
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
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    remaps: Vec::new(),
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
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    remaps: Vec::new(),
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
            session: Default::default(),
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
    /// issue 0794 — the emitter set `NROS_BOOT_SET_NODE_NAME` and nothing else,
    /// so a launch-declared namespace never reached the image while the field,
    /// the bit, the packer and the READER all existed and worked. This asserts
    /// the producer half, in both directions.
    #[test]
    fn a_launch_declared_namespace_reaches_the_baked_boot_config() {
        use std::path::PathBuf;
        fn plan_with(namespace: Option<&str>) -> Plan {
            Plan {
                board: "native".into(),
                nodes: vec![PlanNode {
                    pkg: "talker_pkg".into(),
                    exec: "talker".into(),
                    name: Some("talker".into()),
                    namespace: namespace.map(str::to_string),
                    class_name: None,
                    class_header: None,
                    lang: Some("c".into()),
                    shape: None,
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    remaps: Vec::new(),
                    callback_groups: Vec::new(),
                    sched_context: None,
                    group_tiers: Default::default(),
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
                session: Default::default(),
            }
        }

        let with_ns = boot_config_text(&plan_with(Some("/robot1"))).unwrap();
        assert!(
            with_ns.contains(".namespace_ = \"/robot1\""),
            "the namespace must be baked, not dropped:\n{with_ns}"
        );
        assert!(
            with_ns.contains("NROS_BOOT_SET_NAMESPACE"),
            "the reader branches on the BIT, so baking the field without it \
             changes nothing:\n{with_ns}"
        );

        // The other direction: no namespace declared means the bit stays clear,
        // so the reader falls through to the next rung of RFC-0045's ladder
        // rather than reading an empty string as "configured to root".
        let without = boot_config_text(&plan_with(None)).unwrap();
        assert!(
            !without.contains("NROS_BOOT_SET_NAMESPACE"),
            "an undeclared namespace must not set the bit:\n{without}"
        );
        assert!(without.contains("NROS_BOOT_SET_NODE_NAME"));
    }

    /// issue 0794, the session half — a launch-declared domain, locator and RMW
    /// must reach the blob with their bits set.
    ///
    /// Measured against the writer this replaces, on a bringup whose
    /// `system.toml` declares `domain_id = 7` and `locator =
    /// "tcp/10.0.2.2:7447"` and whose launch pushes `/robot1`:
    ///
    /// ```text
    /// .set_flags  = NROS_BOOT_SET_NODE_NAME | NROS_BOOT_SET_NAMESPACE,
    /// .domain_id  = 0,
    /// .locator    = "",
    /// .rmw        = "",
    /// ```
    ///
    /// The resolver put all three in `execution.deploy./robot1/talker`; the
    /// emitter read that map for the board slice only.
    #[test]
    fn a_launch_declared_session_reaches_the_baked_boot_config() {
        let mut plan = single_node_plan("talker");
        plan.session = BakedSession {
            domain: Some(7),
            locator: Some("tcp/10.0.2.2:7447".into()),
            rmw: Some("zenoh".into()),
            conflicts: Vec::new(),
        };
        let out = boot_config_text(&plan).expect("emit ok");
        for expect in [
            "NROS_BOOT_SET_DOMAIN",
            "NROS_BOOT_SET_LOCATOR",
            "NROS_BOOT_SET_RMW",
            ".domain_id  = 7u",
            ".locator    = \"tcp/10.0.2.2:7447\"",
            ".rmw        = \"zenoh\"",
        ] {
            assert!(out.contains(expect), "missing `{expect}`:\n{out}");
        }

        // The other direction, and the reason the bits exist: nothing declared
        // must leave every session bit CLEAR, so the reader falls through to
        // the next RFC-0045 rung instead of reading domain 0 / `""` as a
        // configured answer.
        let bare = boot_config_text(&single_node_plan("talker")).expect("emit ok");
        for absent in [
            "NROS_BOOT_SET_DOMAIN",
            "NROS_BOOT_SET_LOCATOR",
            "NROS_BOOT_SET_RMW",
        ] {
            assert!(
                !bare.contains(absent),
                "`{absent}` must stay clear:\n{bare}"
            );
        }
        assert!(bare.contains(".domain_id  = 0,"), "{bare}");
        assert!(bare.contains(".locator    = \"\","), "{bare}");
        assert!(bare.contains(".rmw        = \"\","), "{bare}");
    }

    /// issue 0794 — the multi-node decision, stated as a test because it is the
    /// question the fix had to answer.
    ///
    /// A node NAME and a NAMESPACE are per-node identity, so an image running
    /// two nodes has neither and both bits stay clear (unchanged behaviour). A
    /// domain, a locator and an RMW selector are per-SESSION and an image opens
    /// one, so they survive the multi-node case whenever every node agrees.
    #[test]
    fn a_multi_node_image_keeps_its_session_rung_and_drops_its_identity() {
        let mut plan = single_node_plan("talker");
        plan.nodes.push(plan.nodes[0].clone());
        plan.nodes[1].exec = "listener".into();
        plan.session = BakedSession {
            domain: Some(7),
            locator: Some("tcp/10.0.2.2:7447".into()),
            rmw: None,
            conflicts: vec!["rmw".into()],
        };
        let out = boot_config_text(&plan).expect("emit ok");
        assert!(!out.contains("NROS_BOOT_SET_NODE_NAME"), "{out}");
        assert!(!out.contains("NROS_BOOT_SET_NAMESPACE"), "{out}");
        assert!(out.contains("NROS_BOOT_SET_DOMAIN"), "{out}");
        assert!(out.contains("NROS_BOOT_SET_LOCATOR"), "{out}");
        assert!(!out.contains("NROS_BOOT_SET_RMW"), "{out}");
        // A dropped fact is reported in the generated TU, never dropped in
        // silence — the emitted comment is where a reader of the entry looks.
        assert!(
            out.contains("issue 0794 — NOT baked: rmw"),
            "a conflict must be named in the emitted blob:\n{out}"
        );
    }

    /// issue 0794 — the fold from the model's PER-NODE deploy entries to the
    /// blob's ONE session, with every case the emitter has to answer.
    #[test]
    fn baked_session_folds_only_what_every_deployed_node_agrees_on() {
        let s =
            |d, l: Option<&str>, r: Option<&str>| (d, l.map(str::to_string), r.map(str::to_string));

        // Unanimous: the image's answer.
        let got = baked_session(&[
            s(Some(7), Some("tcp/a:7447"), Some("zenoh")),
            s(Some(7), Some("tcp/a:7447"), Some("zenoh")),
        ]);
        assert_eq!(got.domain, Some(7));
        assert_eq!(got.locator.as_deref(), Some("tcp/a:7447"));
        assert_eq!(got.rmw.as_deref(), Some("zenoh"));
        assert!(got.conflicts.is_empty());

        // Disagreement: no answer, and it is REPORTED. A blob holds one domain,
        // so baking either node's would put a fact in the image that the other
        // node contradicts.
        let got = baked_session(&[s(Some(7), None, None), s(Some(9), None, None)]);
        assert_eq!(got.domain, None);
        assert_eq!(got.conflicts, vec!["domain".to_string()]);

        // Partial declaration is a disagreement too: the silent node's answer
        // is "the compiled default", which is not 7.
        let got = baked_session(&[s(Some(7), None, None), s(None, None, None)]);
        assert_eq!(got.domain, None);
        assert_eq!(got.conflicts, vec!["domain".to_string()]);

        // Domain 0 is a real domain, not "unset" — the whole reason the blob
        // carries presence bits rather than reading a zero field as absent.
        let got = baked_session(&[s(Some(0), None, None)]);
        assert_eq!(got.domain, Some(0));
        assert!(got.conflicts.is_empty());

        // An empty string is UNSET, never a configured-empty value: issue 0330
        // for the locator ("absent, let the backend discover"), issue 1050 for
        // the selector ("unset, not a backend named `\"\"`").
        let got = baked_session(&[s(None, Some(""), Some(""))]);
        assert_eq!(got.locator, None);
        assert_eq!(got.rmw, None);
        assert!(
            got.conflicts.is_empty(),
            "an empty value is not a declaration, so it is not a conflict"
        );

        // Nothing declared, nothing reported.
        assert_eq!(
            baked_session(&[s(None, None, None)]),
            BakedSession::default()
        );
        assert_eq!(baked_session(&[]), BakedSession::default());
    }

    /// issue 0794 — a value longer than its fixed C buffer is refused with a
    /// diagnostic naming the field, not passed to the C compiler as an
    /// over-long array initialiser. The budgets differ per field
    /// (`locator[96]`, `rmw[32]`), which the single 63-byte check could not say.
    #[test]
    fn a_session_value_too_long_for_its_c_field_is_refused() {
        let mut plan = single_node_plan("talker");
        plan.session.locator = Some("t".repeat(96));
        let err = boot_config_text(&plan).expect_err("96 bytes does not fit locator[96] + NUL");
        assert!(err.contains("locator"), "{err}");
        assert!(err.contains("95 bytes"), "{err}");

        let mut plan = single_node_plan("talker");
        plan.session.locator = Some("t".repeat(95));
        boot_config_text(&plan).expect("95 bytes fits locator[96] with its NUL");

        let mut plan = single_node_plan("talker");
        plan.session.rmw = Some("r".repeat(32));
        let err = boot_config_text(&plan).expect_err("32 bytes does not fit rmw[32] + NUL");
        assert!(err.contains("rmw"), "{err}");
    }

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
                qos_overrides: Vec::new(),
                params: Vec::new(),
                remaps: Vec::new(),
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
            session: Default::default(),
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
    /// issue 1172 — the shape the two derivations disagreed about: ONE group
    /// id (`ctrl`) named by TWO tiers, because two different nodes each have a
    /// callback group of that name.
    ///
    /// This is the case the corpus never had, which is why the goldens went on
    /// agreeing while the emitters did not. Under the old rules `emit_c`
    /// deduped ACROSS tiers, so tier 1's array came out EMPTY — and an empty
    /// array is the WILDCARD, so that tier stopped filtering and ran every
    /// callback in the image at its own priority. `emit_cpp` kept both.
    ///
    /// The assertions are about the OUTCOME, not the rule: every tier that
    /// names a group gets a non-empty array, and the two tiers' keys differ.
    #[test]
    fn one_group_id_on_two_nodes_is_two_keys_and_neither_tier_is_wildcarded() {
        use crate::orchestration::cargo_metadata_schema::{
            CallbackGroupOverride, NodeOverride, TierDef, TierRtosSpec,
        };
        use std::path::PathBuf;

        let tier_def = |priority: i64| TierDef {
            spin_period_us: Some(10_000),
            posix: Some(TierRtosSpec {
                priority,
                stack_bytes: None,
                preempt_threshold: None,
                time_slice_us: None,
                sched_class: None,
                core: None,
                deadline_us: None,
                budget_us: None,
                period_us: None,
            }),
            ..Default::default()
        };
        let mut tiers = BTreeMap::new();
        tiers.insert("high".to_string(), tier_def(80));
        tiers.insert("low".to_string(), tier_def(10));

        // Both nodes call their group `ctrl`; they sit on different tiers, and
        // one is namespaced, so name alone is not a key either.
        let node = |exec: &str, namespace: Option<&str>| PlanNode {
            pkg: format!("{exec}_pkg"),
            exec: exec.to_string(),
            name: Some(exec.to_string()),
            namespace: namespace.map(str::to_string),
            class_name: None,
            class_header: None,
            lang: Some("c".into()),
            shape: None,
            qos_overrides: Vec::new(),
            params: Vec::new(),
            remaps: Vec::new(),
            callback_groups: vec!["ctrl".into()],
            sched_context: None,
            group_tiers: BTreeMap::new(),
        };

        let mut plan = Plan {
            board: "native".into(),
            nodes: vec![node("fast", None), node("slow", Some("/bay"))],
            depfile_paths: vec![],
            bringup: "demo".into(),
            launch_file: PathBuf::from("/tmp/x.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers,
            node_overrides: vec![
                NodeOverride {
                    name: "fast".to_string(),
                    callback_groups: vec![CallbackGroupOverride {
                        id: "ctrl".to_string(),
                        tier: "high".to_string(),
                    }],
                },
                NodeOverride {
                    name: "slow".to_string(),
                    callback_groups: vec![CallbackGroupOverride {
                        id: "ctrl".to_string(),
                        tier: "low".to_string(),
                    }],
                },
            ],
            resolved_tiers: None,
            session: Default::default(),
        };
        resolve_plan_sched(&mut plan, "posix").expect("resolve_plan_sched");
        let table = plan.resolved_tiers.clone().expect("resolved_tiers");
        assert_eq!(table.tiers.len(), 2, "two tiers");

        let keys = tier_group_keys(&table, &plan);
        for (ti, tier_keys) in keys.iter().enumerate() {
            assert!(
                !tier_keys.is_empty(),
                "tier {ti} names a group, so its array must not be empty — an \
                 empty array is the WILDCARD and would run every callback in \
                 the image at this tier's priority"
            );
        }
        assert_ne!(
            keys[0], keys[1],
            "the two tiers admit DIFFERENT nodes' `ctrl`, so their keys must \
             differ — matching on the group id alone is issue 1172"
        );
        // And the namespace is the node's own, not a default: a filter naming
        // a namespace the node does not have matches nothing.
        let all: Vec<&(String, String, String)> = keys.iter().flatten().collect();
        assert!(
            all.contains(&&("fast".into(), "/".into(), "ctrl".into())),
            "unnamespaced node normalises to `/`; got {all:?}"
        );
        assert!(
            all.contains(&&("slow".into(), "/bay".into(), "ctrl".into())),
            "namespaced node keeps its own namespace; got {all:?}"
        );
    }
}
