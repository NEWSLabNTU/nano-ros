//! Phase 219.B — C++ Entry-pkg TU emitter.
//!
//! Maps a [`Plan`] (see `super::mod`) onto the canonical generated
//! `main.cpp` shape from `docs/roadmap/archived/phase-219-cpp-entry-pkg.md` §3.3.
//!
//! The TU pulls in `<nros/main.hpp>` (the Phase 219.E header that
//! defines the `NROS_MAIN` declarative marker), declares one
//! `extern "C" int32_t __nros_component_<pkg>_register(...)` per
//! launch-XML node, then invokes them in launch order from inside a
//! lambda passed to `nros::board::LinuxBoard::run(...)`.
//!
//! Today the Native-board entry boils down to:
//!
//! - `nros::init()` (no-arg, reads `$NROS_LOCATOR` / `$ROS_DOMAIN_ID`).
//! - Call each Node-pkg register fn in turn — they describe their
//!   `<node>` / `<entity>` set against the supplied `NodeContext`.
//! - `nros::spin()` until `nros::ok()` flips false.
//! - `nros::shutdown()`.
//!
//! The thin `nros::board::<Board>::run(lambda)` adapter shipped by
//! `packages/api/nros-cpp/include/nros/main.hpp` owns the
//! init/spin/shutdown ritual so the generated TU stays one declarative
//! lambda. Phase 235.B added the embedded `ZephyrBoard` sibling: a
//! non-`native` board key (e.g. `"zephyr"`, derived by `nano_ros_entry`
//! from the Phase 215 `NROS_BOARD_RUNNER`) emits
//! `nros::board::ZephyrBoard::run(...)`, which owns the Zephyr + Cyclone
//! `init → network-wait → register → spin → shutdown` lifecycle.

use super::{
    BootConfigView, DeclsView, ExecutorShape, Plan, QosRowView, ServicesView, boot_config_view,
    decls_view, qos_views, sanitize_pkg, services_view,
};

/// The C++ board class an entry calls.
///
/// phase-432 W2.2 — this is a RENDERING of the board family, not a second
/// table. The nineteen board keys collapse onto five families in
/// `nros_entry_lower::board_family`, which is the neutral fact; a `::nros::board::`
/// path is C++'s spelling of it and belongs here, in the emitter, rather than
/// in the lowering. RFC-0091 §8b found the first draft leaking exactly this
/// string into the IR, where a pure-C or Zig pack could not use it.
fn board_cpp_path(board: &str) -> &'static str {
    match family(board) {
        nros_entry_lower::BoardFamily::Native => "::nros::board::LinuxBoard",
        nros_entry_lower::BoardFamily::Zephyr => "::nros::board::ZephyrBoard",
        nros_entry_lower::BoardFamily::Nuttx => "::nros::board::NuttxBoard",
        nros_entry_lower::BoardFamily::Freertos => "::nros::board::FreertosBoard",
        nros_entry_lower::BoardFamily::Threadx => "::nros::board::ThreadxBoard",
    }
}

/// The boot wrapper a generated entry gets.
///
/// phase-432 W2.2 — re-exported from `nros-entry-lower`, which is where the
/// derivation lives now. It was computed here from a board-string match, and
/// the proc-macro computed the same thing a third time; a crate the macro can
/// afford is what lets both stop.
pub(crate) use nros_entry_lower::BootShape;

/// The family of a board key this emitter has already VALIDATED.
///
/// Issue 1285 — `board_family` refuses an unknown key rather than answering
/// `Native`. Every public entry point funnels through `emit_typed_with_tail`,
/// which refuses an unknown `plan.board` with the known-keys message before
/// any helper below runs, so a miss here is a broken invariant, not user input.
fn family(board: &str) -> nros_entry_lower::BoardFamily {
    nros_entry_lower::board_family(board)
        .unwrap_or_else(|e| panic!("emit_cpp: board not validated at entry: {e}"))
}

/// The boot shape for a board key.
pub(crate) fn boot_shape(board: &str) -> BootShape {
    family(board).boot_shape()
}

pub(crate) fn board_is_embedded(board: &str) -> bool {
    family(board).is_embedded()
}

/// phase-263 C2d — Zephyr is the exception among embedded boards: the Zephyr kernel
/// calls the application's `main()` DIRECTLY (there is no nano-ros `startup.c` owning
/// `main`), so a Zephyr LAUNCH entry emits a plain `int main(void)` driving
/// `ZephyrBoard::run_components` — NOT the `nros_app_main` + `NROS_APP_MAIN_REGISTER_VOID`
/// shape the FreeRTOS/NuttX/ThreadX startup paths require. The entry TU is added to the
/// Zephyr `app` target (`nano_ros_entry` zephyr branch), and the connect locator threads
/// in via the compile-time `CONFIG_NROS_ZENOH_LOCATOR` Kconfig (read through the
/// `NROS_ENTRY_LOCATOR` default in `<nros/main.hpp>`), not a baked `-D`.
pub(crate) fn board_is_zephyr(board: &str) -> bool {
    family(board) == nros_entry_lower::BoardFamily::Zephyr
}

// issue 1283 — `board_is_freertos_embedded` / `board_is_nuttx` existed only to
// spell "this board has `run_tiers`" inside this pack's branch predicate. That
// predicate is the plan's now (`Plan::executor_shape`, shared with the C pack),
// and the board half of it is `board_has_run_tiers` in `mod.rs`.

/// Phase 240.2 (RFC-0043) — **typed** entry emitter. Routes each launch node to
/// the REAL executor via its component object, instead of the legacy type-erased
/// `__nros_component_<pkg>_register` call into the synthesizing
/// `EntryNodeRuntime`. Per node: `#include` the component header, declare static
/// component + node storage (outlives the spin loop — the executor holds
/// `&component` as the dispatch context; no heap), construct the node + call
/// `component.configure(node)` (binds the real callbacks). `main` hands the setup
/// fn to `Board::run_components` (init → setup → `spin_once` loop → shutdown).
///
/// Each node is routed by its `lang` (Phase 240.4): a **C++** node needs
/// `class_name` + `class_header` (construct the class, call `configure(node)`);
/// a **C** node (`lang == "c"`) needs only its pkg — it is built via the C-ABI
/// factory + configure seam `__nros_c_component_<pkg>_{create,configure}`
/// (`NROS_C_COMPONENT`), to which the entry hands the node's `ffi_handle()`.
/// Returns an error naming the offending pkg on a missing requirement.
/// phase-308 W1 — what the generated TU does after `__nros_entry_setup` has
/// constructed and configured every node.
///
/// The setup body is identical for a real entry and for a metadata probe: both
/// need the same per-node includes, static storage, construction and
/// `configure` call, across all three component shapes (C++ `configure`, the C
/// ABI seam, rclcpp construct-with-handle). Only the tail differs — an entry
/// spins, a probe records and exits.
///
/// Splitting here rather than writing a second emitter is deliberate: a
/// parallel emitter would fork three shape-handlers, and the count those
/// probes produce is exactly what must not drift between them.
pub enum EntryTail<'a> {
    /// Hand the setup fn to `Board::run_components` (init → setup → spin →
    /// shutdown). The shipping entry.
    Board,
    /// Open a session against the recording RMW backend, run setup once, dump
    /// the recorded metadata, exit. Never spins.
    MetadataProbe(&'a ProbeExport),
}

/// phase-308 W1 — identity the probe stamps into the sidecar it writes.
pub struct ProbeExport {
    pub package: String,
    pub component: String,
    pub executable: String,
    /// `"c"` or `"cpp"` — the sidecar's `language` field.
    pub language: String,
    /// Absolute path the probe writes the sidecar to.
    pub out_path: String,
}

pub fn emit_typed(plan: &Plan) -> Result<String, String> {
    emit_typed_with_tail(plan, &EntryTail::Board)
}

// ---------------------------------------------------------------------------
// phase-462 W1 (RFC-0052) -- the contract monitor table region.
//
// The Rust road bakes `model_ingest::monitor_rows` / `age_rows` into
// `system_monitors.rs` (`render_monitor_rs`); this is the same rows on the
// C++ road. The entry declares them as `nros_cpp_monitor_row_t` /
// `nros_cpp_age_row_t` statics plus the storage the runtime builds its own
// spec/cell tables in, and installs them through `nros_cpp_install_monitors`
// at the top of every setup function, BEFORE any node is created -- the
// order `Executor::set_monitor_table` requires, because the publisher cell
// attaches at `create_publisher`. No rows: no statics, no call, and the TU is
// byte-identical to what it was before this region existed.
//
// Two slices happen here and nowhere else. The model's rows cover every node
// of the system, so a row is kept only when its node (`fqn` minus the
// endpoint) is one this entry constructs; and in the run_tiers shape each
// tier's setup installs only its own nodes' rows on its own executor, since a
// row installed on two executors would be checked -- and reported -- twice.
// ---------------------------------------------------------------------------

/// phase-462 W1 -- the shipping typed entry WITH the contract monitor table.
/// `emit_typed` is this with no rows.
pub fn emit_typed_monitored(
    plan: &Plan,
    monitors: &[crate::orchestration::model_ingest::MonitorRow],
    ages: &[crate::orchestration::model_ingest::AgeRow],
) -> Result<String, String> {
    emit_typed_with_tail_monitored(plan, &EntryTail::Board, monitors, ages)
}

/// One baked monitor table: the rows for one executor (the whole entry in
/// the single-executor shape; one tier's in the run_tiers shape).
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct CppMonitorTableView {
    /// Symbol suffix (`""` for the single table, `"_t<i>"` per tier).
    tag: String,
    /// Prose for the banner comment (`" (tier[1] telem)"` or empty).
    where_: String,
    rows: Vec<CppMonitorRowView>,
    ages: Vec<CppAgeRowView>,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct CppMonitorRowView {
    /// RAW; the pack quotes it with `c_str`.
    topic: String,
    fqn: String,
    min_rate_hz_milli: u32,
    max_latency_ms: u32,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq)]
struct CppAgeRowView {
    topic: String,
    fqn: String,
    max_age_ms: u32,
}

impl CppMonitorTableView {
    fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.ages.is_empty()
    }
}

/// The node FQN a plan node registers under (`<namespace>/<name>`), the key
/// a contract row's `fqn` starts with.
fn plan_node_fqn(n: &super::PlanNode) -> String {
    let name = n.name.as_deref().unwrap_or(&n.exec);
    // Issue 1443 — the namespace half is `super::node_namespace`, the one
    // derivation, so a contract row's key and the `create_node` call the entry
    // renders cannot disagree about what "no namespace" means.
    match super::node_namespace(n) {
        "/" => format!("/{name}"),
        ns => format!("{}/{name}", ns.trim_end_matches('/')),
    }
}

/// The node half of an endpoint ref (`/ns/node/endpoint` -> `/ns/node`).
fn row_node_fqn(endpoint_ref: &str) -> &str {
    endpoint_ref
        .rsplit_once('/')
        .map(|(node, _)| node)
        .unwrap_or("")
}

/// Build the table for the nodes `keep` admits, in row order.
fn monitor_table_view(
    tag: &str,
    where_: &str,
    monitors: &[crate::orchestration::model_ingest::MonitorRow],
    ages: &[crate::orchestration::model_ingest::AgeRow],
    keep: impl Fn(&str) -> bool,
) -> CppMonitorTableView {
    CppMonitorTableView {
        tag: tag.to_string(),
        where_: where_.to_string(),
        rows: monitors
            .iter()
            .filter(|r| keep(row_node_fqn(&r.fqn)))
            .map(|r| CppMonitorRowView {
                topic: r.topic.clone(),
                fqn: r.fqn.clone(),
                min_rate_hz_milli: r.min_rate_hz_milli,
                max_latency_ms: r.max_latency_ms,
            })
            .collect(),
        ages: ages
            .iter()
            .filter(|r| keep(row_node_fqn(&r.fqn)))
            .map(|r| CppAgeRowView {
                topic: r.topic.clone(),
                fqn: r.fqn.clone(),
                max_age_ms: r.max_age_ms,
            })
            .collect(),
    }
}

/// The single-executor table: every row whose node this entry constructs.
fn monitor_table_single(
    plan: &Plan,
    monitors: &[crate::orchestration::model_ingest::MonitorRow],
    ages: &[crate::orchestration::model_ingest::AgeRow],
) -> Option<CppMonitorTableView> {
    let nodes: Vec<String> = plan.nodes.iter().map(plan_node_fqn).collect();
    let t = monitor_table_view("", "", monitors, ages, |node| {
        nodes.iter().any(|n| n == node)
    });
    (!t.is_empty()).then_some(t)
}

/// The per-tier tables: tier `ti` gets the rows of the nodes its setup
/// constructs (the same `node -> tier` map the setups are built from).
fn monitor_tables_tiered(
    plan: &Plan,
    tiers: &super::ResolvedTierTable,
    monitors: &[crate::orchestration::model_ingest::MonitorRow],
    ages: &[crate::orchestration::model_ingest::AgeRow],
) -> Vec<Option<CppMonitorTableView>> {
    tiers
        .tiers
        .iter()
        .enumerate()
        .map(|(ti, tier)| {
            let nodes: Vec<String> = plan
                .nodes
                .iter()
                .filter(|n| {
                    let node_name = n.name.as_deref().unwrap_or(&n.exec);
                    tier.members.iter().any(|(m, _)| m == node_name)
                })
                .map(plan_node_fqn)
                .collect();
            let t = monitor_table_view(
                &format!("_t{ti}"),
                &format!(" (tier[{ti}] {})", tier.name),
                monitors,
                ages,
                |node| nodes.iter().any(|n| n == node),
            );
            (!t.is_empty()).then_some(t)
        })
        .collect()
}

/// phase-308 W1 — the metadata probe: the same TU an entry would be, with a
/// recording tail.
pub fn emit_typed_probe(plan: &Plan, export: &ProbeExport) -> Result<String, String> {
    emit_typed_with_tail(plan, &EntryTail::MetadataProbe(export))
}

/// The whole TU, as the template sees it.
///
/// Issue 1102 / phase-432 W2.3 — every field is ALREADY CORRECT. Board paths
/// come from `nros_entry_lower`, tier rows are VALUES rather than initialiser
/// text, and raw strings stay raw so the pack quotes them with its own filter
/// (RFC-0091 §8b). Three fields are still pre-rendered and say why below.
#[derive(serde::Serialize)]
struct CppEntryView {
    bringup: String,
    launch: String,
    board: String,
    /// phase-263 C2 — embedded boots through the board's `startup.c`; Zephyr
    /// is exempt because its kernel calls `main()` directly.
    app_main_include: bool,
    /// RFC-0044 — pulled in only when an rclcpp-shape node is present.
    rclcpp_include: bool,
    /// Unique C++ component headers, first-seen order. Deduped by HEADER while
    /// storage is per NODE, so this list and `storage` differ in length
    /// whenever two nodes share a header.
    headers: Vec<String>,
    /// Unique sanitized package symbols for the C factory/configure seam, and
    /// for the Rust install seam. Deduped by PACKAGE, same reason.
    c_pkgs: Vec<String>,
    rust_pkgs: Vec<String>,
    storage: Vec<CppStorageView>,
    tiers: Option<CppTiersView>,
    /// Single-executor path only, and only when the plan declares tiers this
    /// board cannot run as tasks.
    sched: Option<super::SchedView>,
    /// Single-executor path only; empty when `tiers` is set.
    setup_nodes: Vec<CppNodeView>,
    /// phase-462 W1 -- the single-executor contract monitor table, installed
    /// at the top of `__nros_entry_setup`. `None` when the entry has no rows.
    monitors: Option<CppMonitorTableView>,
    /// phase-462 W1 -- every non-empty table (the single one, or one per
    /// tier), for the file-scope row and storage statics.
    monitor_tables: Vec<CppMonitorTableView>,
    /// The param-services / lifecycle facts. The template includes
    /// `cpp_service_trailer.cpp.jinja` where they belong, with the `tiered`
    /// flag that picks the executor expression.
    services: ServicesView,
    /// phase-308 — set for a metadata probe, which returns before the boot
    /// config and the board wrapper.
    probe: Option<CppProbeView>,
    /// `None` for a probe, which returns before the blob. Rendered by the
    /// shared `boot_config.c.jinja` — the same partial the C pack includes.
    boot_config: Option<BootConfigView>,
    boot: Option<CppBootView>,
}

/// One node's static storage. A Rust node has none — it self-creates its node
/// on the shared executor — so it contributes no row at all rather than an
/// empty one.
#[derive(serde::Serialize)]
struct CppStorageView {
    index: usize,
    /// `"rclcpp"` gets an aligned arena slot; anything else gets a
    /// `::rclcpp::Node`, plus a component object when `class` is set (a C node
    /// keeps its state in its own TU, so it has none).
    shape: &'static str,
    class: Option<String>,
}

#[derive(serde::Serialize)]
struct CppTiersView {
    n: usize,
    setups: Vec<CppTierSetupView>,
    /// One row per tier, STRUCTURED. The same neutral rows the C pack renders
    /// as `nros_native_tier_spec_t` — shared so the two packs cannot spell one
    /// tier two ways (that is what W1.2's gate exists to catch one layer down).
    tiers: Vec<super::TierView>,
}

#[derive(serde::Serialize)]
struct CppTierSetupView {
    index: usize,
    name: String,
    /// phase-462 W1 -- this tier's contract monitor rows, installed on its
    /// executor at the top of its setup. `None` when it has none.
    monitors: Option<CppMonitorTableView>,
    nodes: Vec<CppNodeView>,
    /// Only tier 0 registers param services and lifecycle.
    emits_services: bool,
}

#[derive(serde::Serialize)]
struct CppNodeView {
    index: usize,
    /// RAW. The pack quotes it with `c_str`.
    name: String,
    /// RAW node namespace, from `super::node_namespace` — issue 1443. The
    /// `create_node` / `create_node_on` calls used to pass NAME ONLY, and the
    /// default argument on both is `nullptr`, which `nros_cpp_node_create`
    /// substitutes `"/"` for. Never empty (the derivation normalises "none" to
    /// `"/"`), so the same value is safe in every arity.
    namespace: String,
    /// Issue 1456 — what the LAUNCH FILE declared, `None` when it declared
    /// nothing. Deliberately NOT `name` / `namespace` above: those two are
    /// resolved for a caller that must supply an answer (`name` falls back to
    /// `exec`, `namespace` normalises "none" to `"/"`), and the `rclcpp` shape
    /// needs the opposite — "did the launch file state one?", because when it
    /// did not, the component CLASS's own literal is the answer and a resolved
    /// value would silently outrank it. Reaches the `::nros::NodeHandle`
    /// constructor; `None` renders `nullptr`.
    launch_name: Option<String>,
    launch_namespace: Option<String>,
    /// `"c"` | `"rust"` | `"rclcpp"` | `"configure"`.
    shape: &'static str,
    pkg: String,
    class: Option<String>,
    /// Whether this body renders inside a per-tier setup function, which is
    /// what selects the executor expression. A fact about WHERE it renders,
    /// not about the node.
    tiered: bool,
    /// The remap + param calls, rendered by the SHARED
    /// `declare_calls.c.jinja` — the same partial the C pack includes.
    decls: DeclsView,
    /// The QoS overrides. NOT shared: this pack calls a method on the node
    /// where C calls a free function on its address.
    qos: Vec<QosRowView>,
}

#[derive(serde::Serialize)]
struct CppProbeView {
    package: String,
    component: String,
    executable: String,
    language: String,
    out_path: String,
}

#[derive(serde::Serialize)]
struct CppBootView {
    /// `"kernel"` | `"app"` | `"host"` — `BootShape`'s single derivation.
    shape: &'static str,
    board_path: &'static str,
    tiers: bool,
    n_tiers: usize,
}

fn boot_shape_str(shape: BootShape) -> &'static str {
    match shape {
        BootShape::Kernel => "kernel",
        BootShape::App => "app",
        BootShape::Host => "host",
    }
}

/// Which of the four construction shapes a node takes.
fn node_shape(n: &super::PlanNode) -> &'static str {
    if is_c_node(n) {
        "c"
    } else if is_rust_node(n) {
        "rust"
    } else if is_rclcpp_node(n) {
        "rclcpp"
    } else {
        "configure"
    }
}

/// `on_executor` is the node's index on the executor its setup function
/// builds it on (issue 1272): the tier's position for a tiered setup, the plan
/// position for the single one.
fn node_view(n: &super::PlanNode, i: usize, on_executor: usize, tiered: bool) -> CppNodeView {
    let exec_expr = if tiered {
        "executor"
    } else {
        "::nros::global_handle()"
    };
    CppNodeView {
        index: i,
        name: n.name.as_deref().unwrap_or(&n.exec).to_string(),
        namespace: super::node_namespace(n).to_string(),
        // Issue 1456 — the RAW plan values, with an empty string treated as
        // "not declared" the same way `node_namespace` does. `filter` rather
        // than `map`, so `name=""` in a launch file cannot reach the handle
        // and blank a component's name.
        launch_name: n
            .name
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        launch_namespace: n
            .namespace
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        shape: node_shape(n),
        pkg: sanitize_pkg(&n.pkg),
        class: n.class_name.clone(),
        tiered,
        decls: decls_view(n, exec_expr, on_executor),
        qos: qos_views(n),
    }
}

pub fn emit_typed_with_tail(plan: &Plan, tail: &EntryTail<'_>) -> Result<String, String> {
    emit_typed_with_tail_monitored(plan, tail, &[], &[])
}

/// phase-462 W1 -- `emit_typed_with_tail` plus the contract monitor rows.
pub fn emit_typed_with_tail_monitored(
    plan: &Plan,
    tail: &EntryTail<'_>,
    monitors: &[crate::orchestration::model_ingest::MonitorRow],
    ages: &[crate::orchestration::model_ingest::AgeRow],
) -> Result<String, String> {
    // Issue 1285 — refuse an unknown board key HERE, naming the known ones.
    // Every board helper in this module relies on it (see `family`).
    nros_entry_lower::board_family(&plan.board).map_err(|e| format!("typed entry emit: {e}"))?;
    for n in &plan.nodes {
        // phase-432 W2.6 — the exemption is the SAME one the `class_header`
        // guard below already applies, and it was missing here alone.
        //
        // A C or Rust node has no C++ class: `storage` sets `class: None` for a
        // C node explicitly, and a Rust node contributes no storage at all. So
        // this required cmake to have filled in a field the emitter then
        // discards — checkable only because the launch path happens to carry
        // metadata for every node. `nano_ros_node_register`'s C components do
        // not, and inventing a class name to satisfy a guard that throws it
        // away would be a fact with no referent.
        if n.class_name.is_none() && !is_c_node(n) && !is_rust_node(n) {
            return Err(format!(
                "typed entry emit: node pkg `{}` exec `{}` is missing class_name (cmake metadata)",
                n.pkg, n.exec
            ));
        }
        if !is_c_node(n) && !is_rust_node(n) && n.class_header.is_none() {
            return Err(format!(
                "typed entry emit: C++ node pkg `{}` exec `{}` is missing class_header \
                 — the typed Entry needs the component's class header (cmake metadata)",
                n.pkg, n.exec
            ));
        }
    }

    // One `#include` per unique C++ component header (first-seen order). C and
    // Rust nodes carry none — their seams are `extern "C"` declarations.
    let mut headers: Vec<String> = Vec::new();
    for n in &plan.nodes {
        if is_c_node(n) || is_rust_node(n) {
            continue;
        }
        let h = n.class_header.as_deref().unwrap().to_string();
        if !headers.contains(&h) {
            headers.push(h);
        }
    }

    let mut c_pkgs: Vec<String> = Vec::new();
    let mut rust_pkgs: Vec<String> = Vec::new();
    for n in &plan.nodes {
        let pkg = sanitize_pkg(&n.pkg);
        if is_c_node(n) {
            if !c_pkgs.contains(&pkg) {
                c_pkgs.push(pkg);
            }
        } else if is_rust_node(n) && !rust_pkgs.contains(&pkg) {
            rust_pkgs.push(pkg);
        }
    }

    // Static per-node storage. Shape-branched (RFC-0044): an rclcpp component
    // OWNS its node, so it gets an arena slot and no `::rclcpp::Node`; a C node
    // keeps its state in its own TU, so it gets no component object; a Rust
    // node self-creates and gets nothing at all.
    let storage: Vec<CppStorageView> = plan
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| !is_rust_node(n))
        .map(|(i, n)| CppStorageView {
            index: i,
            shape: node_shape(n),
            class: if is_c_node(n) {
                None
            } else {
                n.class_name.clone()
            },
        })
        .collect();

    // issue 1283 — the branch is the PLAN's (`Plan::executor_shape`), shared
    // with the C pack: a multi-tier plan takes `run_tiers` unless a node's
    // groups span tiers (RFC-0047 group split — per-tier setups construct whole
    // NODES) or the board has no `run_tiers` (ThreadX, issue 1286); those keep
    // the single-executor sched-context path.
    //
    // phase-308 W1 — the one refinement is ours alone: a metadata probe always
    // takes the single-setup shape. Tiers would be worse than irrelevant:
    // `create_entity` early-returns for entities whose callback group is
    // inactive on the running tier, so a per-tier probe would UNDER-count
    // exactly what the sidecar exists to count.
    let shape = match (plan.executor_shape(), tail) {
        (ExecutorShape::Tiers, EntryTail::MetadataProbe(_)) => ExecutorShape::SchedContexts,
        (shape, _) => shape,
    };
    let use_run_tiers = shape == ExecutorShape::Tiers;

    let mut tiers_view: Option<CppTiersView> = None;
    let mut sched_view: Option<super::SchedView> = None;
    let mut setup_nodes: Vec<CppNodeView> = Vec::new();
    // phase-462 W1 -- the monitor table(s): one per tier setup, or one for
    // the single executor. `monitor_tables` is every non-empty one, for the
    // file-scope statics; each setup names its own by `tag`.
    let mut single_monitors: Option<CppMonitorTableView> = None;
    let mut monitor_tables: Vec<CppMonitorTableView> = Vec::new();

    if use_run_tiers {
        let tiers = plan.resolved_tiers.as_ref().unwrap();

        // #0266 — `time_slice_us` has a per-thread consumer only on the Rust
        // ThreadX arm today; the C++ tier ABI carries no field. Fail loud
        // rather than silently drop a declared value on a C++ bake.
        if let Some(t) = tiers.tiers.iter().find(|t| t.time_slice_us.is_some()) {
            return Err(format!(
                "tier '{}': time_slice_us is not yet supported on the C/C++ codegen \
                 path (#0266) — declare it only on a Rust `nros::main!` ThreadX entry, \
                 or file the C consumer",
                t.name
            ));
        }

        // node_name → tier index, for per-tier node filtering.
        let node_to_tier: std::collections::HashMap<&str, usize> = tiers
            .tiers
            .iter()
            .enumerate()
            .flat_map(|(ti, tier)| {
                tier.members
                    .iter()
                    .map(move |(node_name, _group)| (node_name.as_str(), ti))
            })
            .collect();

        let mut tier_monitors = monitor_tables_tiered(plan, tiers, monitors, ages);
        let setups: Vec<CppTierSetupView> = tiers
            .tiers
            .iter()
            .enumerate()
            .map(|(ti, tier)| CppTierSetupView {
                index: ti,
                name: tier.name.clone(),
                monitors: tier_monitors[ti].take(),
                nodes: plan
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| {
                        let node_name = n.name.as_deref().unwrap_or(&n.exec);
                        node_to_tier.get(node_name).copied() == Some(ti)
                    })
                    // issue 1272 -- each tier setup builds its nodes on the
                    // tier's OWN executor, so the index restarts per tier.
                    .enumerate()
                    .map(|(k, (i, n))| node_view(n, i, k, true))
                    .collect(),
                emits_services: ti == 0,
            })
            .collect();

        // issue 1172 — ONE derivation now, and it lives in `mod.rs` because the
        // two packs used to disagree here: `emit_c` deduped ACROSS tiers and
        // `emit_cpp` within each tier, so the same plan produced two different
        // filters. With the node in the key there is nothing to dedup across
        // tiers, and the rule that is left is the same for both.
        let groups_per_tier = super::tier_group_keys(tiers, plan);

        monitor_tables = setups.iter().filter_map(|s| s.monitors.clone()).collect();
        tiers_view = Some(CppTiersView {
            n: tiers.tiers.len(),
            setups,
            tiers: super::tier_views(tiers, groups_per_tier),
        });
    } else {
        if shape == ExecutorShape::SchedContexts {
            // issue 1283 — ONE derivation, shared with the C pack.
            let tiers = plan.resolved_tiers.as_ref().unwrap();
            sched_view = Some(super::sched_view(tiers, plan));
        }

        setup_nodes = plan
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| node_view(n, i, i, false))
            .collect();
        single_monitors = monitor_table_single(plan, monitors, ages);
        monitor_tables.extend(single_monitors.iter().cloned());
    }

    let probe = match tail {
        EntryTail::MetadataProbe(e) => Some(CppProbeView {
            package: e.package.clone(),
            component: e.component.clone(),
            executable: e.executable.clone(),
            language: e.language.clone(),
            out_path: e.out_path.clone(),
        }),
        EntryTail::Board => None,
    };

    // A probe returns before the boot config and the board wrapper: it opens a
    // session against the recording backend, runs setup once, and dumps.
    let (boot_config, boot) = if probe.is_some() {
        (None, None)
    } else {
        (
            Some(boot_config_view(plan)?),
            Some(CppBootView {
                shape: boot_shape_str(boot_shape(&plan.board)),
                board_path: board_cpp_path(&plan.board),
                tiers: use_run_tiers,
                n_tiers: plan
                    .resolved_tiers
                    .as_ref()
                    .map(|t| t.tiers.len())
                    .unwrap_or(0),
            }),
        )
    };

    super::render::render(
        "entry_cpp.cpp",
        &CppEntryView {
            bringup: plan.bringup.clone(),
            launch: plan.launch_file.display().to_string(),
            board: plan.board.clone(),
            app_main_include: board_is_embedded(&plan.board) && !board_is_zephyr(&plan.board),
            rclcpp_include: plan.nodes.iter().any(is_rclcpp_node),
            headers,
            c_pkgs,
            rust_pkgs,
            storage,
            tiers: tiers_view,
            sched: sched_view,
            setup_nodes,
            monitors: single_monitors,
            monitor_tables,
            services: services_view(plan),
            probe,
            boot_config,
            boot,
        },
    )
}

fn is_c_node(n: &super::PlanNode) -> bool {
    n.lang.as_deref() == Some("c")
}

/// Phase 257 (W0-B) — a `lang == "rust"` node is installed via the uniform
/// `__nros_component_<pkg>_install` seam onto the shared executor; it self-creates
/// its node (no entry-created `::rclcpp::Node`, no C++ class, no qos-override — D7
/// Option C).
fn is_rust_node(n: &super::PlanNode) -> bool {
    n.lang.as_deref() == Some("rust")
}

/// Phase 242.4 (RFC-0044) — an rclcpp-shape (IS-A-node, construct-with-handle)
/// C++ component: `shape == "rclcpp"` AND not a C node. Everything else (incl.
/// `shape == None` / `"configure"`) keeps the 240.x `configure(Node&)` path.
fn is_rclcpp_node(n: &super::PlanNode) -> bool {
    !is_c_node(n) && n.shape.as_deref() == Some("rclcpp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::entry::PlanNode;
    use std::path::PathBuf;

    fn fixture_plan(nodes: &[(&str, &str)]) -> Plan {
        Plan {
            board: "native".into(),
            nodes: nodes
                .iter()
                .map(|(pkg, exec)| PlanNode {
                    pkg: (*pkg).into(),
                    exec: (*exec).into(),
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
                    group_tiers: std::collections::BTreeMap::new(),
                })
                .collect(),
            depfile_paths: Vec::new(),
            bringup: "demo_bringup".into(),
            launch_file: PathBuf::from("/tmp/system.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers: Default::default(),
            node_overrides: Vec::new(),
            resolved_tiers: None,
            session: Default::default(),
        }
    }

    /// Typed-emit fixture: each tuple is `(pkg, exec, name, class, header)`.
    /// Defaults to the `configure(Node&)` shape (240.x); use
    /// [`fixture_plan_rclcpp`] for the construct-with-handle shape.
    fn fixture_plan_typed(nodes: &[(&str, &str, &str, &str, &str)]) -> Plan {
        Plan {
            board: "native".into(),
            nodes: nodes
                .iter()
                .map(|(pkg, exec, name, class, header)| PlanNode {
                    pkg: (*pkg).into(),
                    exec: (*exec).into(),
                    name: Some((*name).into()),
                    namespace: None,
                    class_name: Some((*class).into()),
                    class_header: Some((*header).into()),
                    lang: Some("cpp".into()),
                    shape: Some("configure".into()),
                    qos_overrides: Vec::new(),
                    params: Vec::new(),
                    remaps: Vec::new(),
                    callback_groups: Vec::new(),
                    sched_context: None,
                    group_tiers: std::collections::BTreeMap::new(),
                })
                .collect(),
            depfile_paths: Vec::new(),
            bringup: "demo_bringup".into(),
            launch_file: PathBuf::from("/tmp/system.launch.xml"),
            lifecycle: None,
            param_services: false,
            safety: None,
            tiers: Default::default(),
            node_overrides: Vec::new(),
            resolved_tiers: None,
            session: Default::default(),
        }
    }

    /// Phase 242.4 — rclcpp-shape typed fixture: same tuple as
    /// [`fixture_plan_typed`] but `shape == "rclcpp"` (construct-with-handle).
    fn fixture_plan_rclcpp(nodes: &[(&str, &str, &str, &str, &str)]) -> Plan {
        let mut plan = fixture_plan_typed(nodes);
        for n in &mut plan.nodes {
            n.shape = Some("rclcpp".into());
        }
        plan
    }

    /// phase-308 W1 — the probe is the SAME TU an entry would be, minus the
    /// board and plus a dump. That is the point: the per-node construction and
    /// `configure` calls come from one emitter, so the entity count a probe
    /// records cannot drift from what the real entry registers.
    #[test]
    fn metadata_probe_reuses_the_setup_body_and_swaps_the_tail() {
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let export = ProbeExport {
            package: "talker_pkg".into(),
            component: "talker".into(),
            executable: "talker".into(),
            language: "cpp".into(),
            out_path: "/ws/src/talker_pkg/metadata/talker.json".into(),
        };
        let src = emit_typed_probe(&plan, &export).expect("probe emit ok");

        // Same setup body as the entry: header, construction, configure.
        assert!(src.contains("#include \"talker_pkg/Talker.hpp\""), "{src}");
        assert!(src.contains("__nros_entry_setup"), "{src}");
        assert!(src.contains(".configure("), "{src}");

        // Probe tail: dump, with the identity the sidecar is stamped with.
        assert!(
            src.contains("nros_cpp_metadata_dump(\"talker_pkg\", \"talker\", \"talker\", \"cpp\""),
            "{src}"
        );
        assert!(
            src.contains("/ws/src/talker_pkg/metadata/talker.json"),
            "{src}"
        );
        // Explicit registration is what pulls the backend object out of the
        // static archive; with no reference the linker omits it entirely.
        assert!(src.contains("nros_rmw_metadata_register()"), "{src}");

        // NOT an entry: no board CALL, no spin, no boot-config blob. Match the
        // call form — the generated file's header comment mentions
        // `Board::run_components` prose, which a bare substring test flags.
        assert!(
            !src.contains("::run_components("),
            "probe must not spin:\n{src}"
        );
        assert!(!src.contains("NROS_BOOT_CONFIG"), "{src}");
        assert!(
            !src.contains("run_tiers"),
            "probe records every tier:\n{src}"
        );
    }

    #[test]
    fn typed_emit_includes_headers_constructs_and_runs_components() {
        let plan = fixture_plan_typed(&[
            (
                "talker_pkg",
                "talker",
                "talker",
                "talker_pkg::Talker",
                "talker_pkg/Talker.hpp",
            ),
            (
                "listener_pkg",
                "listener",
                "listener",
                "listener_pkg::Listener",
                "listener_pkg/Listener.hpp",
            ),
        ]);
        let src = emit_typed(&plan).expect("typed emit ok");
        // headers included (including boot_config.h for the node-name blob)
        assert!(src.contains("#include <nros/boot_config.h>"));
        assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
        assert!(src.contains("#include \"listener_pkg/Listener.hpp\""));
        assert!(src.contains("#include <nros/component.hpp>"));
        // static component + node storage
        assert!(src.contains("static ::rclcpp::Node __nros_node_0;"));
        assert!(src.contains("static ::talker_pkg::Talker __nros_comp_0;"));
        assert!(src.contains("static ::listener_pkg::Listener __nros_comp_1;"));
        // setup constructs the node + configures the component
        assert!(src.contains("::nros::create_node(__nros_node_0, \"talker\", \"/\")"));
        assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
        assert!(src.contains("__nros_comp_1.configure(__nros_node_1)"));
        // routes to the real executor via the named overload (phase 266)
        assert!(src.contains(
            "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
        ));
        assert!(!src.contains("__nros_component_"));
        assert!(!src.contains("NodeContext"));
        // configure shape: no construct-with-handle artifacts.
        assert!(!src.contains("global_handle()"));
        assert!(!src.contains("__nros_comp_buf_"));
        // multi-node: boot config must be all-unset (no single node name baked)
        assert!(src.contains(".set_flags  = 0,"));
        assert!(!src.contains("NROS_BOOT_SET_NODE_NAME"));
    }

    // Phase 305 W3 (issue 0255) — launch `<remap>` rules bake as per-pair
    // `nros_cpp_declare_remap` calls BEFORE construction/configure (rclcpp
    // ctors register entities immediately; configure-shape registers there).
    #[test]
    fn typed_emit_remaps_declared_before_configure() {
        let mut plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        plan.nodes[0].remaps = vec![("chatter".into(), "chatter_remapped".into())];
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(
            src.contains(
                "nros_cpp_declare_remap(::nros::global_handle(), \"talker\", \"/\", \"chatter\", \"chatter_remapped\")"
            ),
            "expected declare_remap call; src:\n{src}"
        );
        let remap_at = src.find("nros_cpp_declare_remap").unwrap();
        let cfg_at = src.find(".configure(__nros_node_0)").unwrap();
        assert!(remap_at < cfg_at, "remap decl must precede configure");
    }

    #[test]
    fn typed_emit_no_remaps_no_declare_calls() {
        // Guard: remap-free plans produce byte-identical output.
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(!src.contains("nros_cpp_declare_remap"));
    }

    /// Phase 211.H (issue #52) — a configure-shape node carrying qos_overrides
    /// emits the static `nros_cpp_qos_override_t[]` table + a `set_qos_overrides`
    /// call BEFORE `configure`, with the role/policy/value mapped to C-ABI codes.
    #[test]
    fn typed_emit_bakes_qos_overrides_before_configure() {
        let mut plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        // Built through the shared lowering, so the test cannot assert codes
        // the real bake would never produce.
        plan.nodes[0].qos_overrides = nros_orchestration_ir::qos_override::lower_all([
            (
                "qos_overrides./chatter.publisher.reliability",
                "best_effort",
            ),
            (
                "qos_overrides./chatter.subscription.durability",
                "transient_local",
            ),
        ])
        .expect("fixture overrides lower");
        let src = emit_typed(&plan).expect("typed emit ok");

        // Static table with the two overrides, C-ABI codes:
        //   publisher(0)/reliability(0)/best_effort(0); subscription(1)/durability(1)/transient_local(1)
        assert!(src.contains("static const ::nros_cpp_qos_override_t __nros_qos_0[] = {"));
        assert!(src.contains("{ \"/chatter\", 0, 0, 0 }"));
        assert!(src.contains("{ \"/chatter\", 1, 1, 1 }"));
        // Installed on the node, and BEFORE configure.
        assert!(src.contains("__nros_node_0.set_qos_overrides(__nros_qos_0, 2)"));
        let set_at = src.find("set_qos_overrides").unwrap();
        let cfg_at = src.find("__nros_comp_0.configure(__nros_node_0)").unwrap();
        assert!(set_at < cfg_at, "set_qos_overrides must precede configure");
    }

    /// A node with no qos_overrides emits no table / set call.
    #[test]
    fn typed_emit_no_qos_overrides_no_table() {
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(!src.contains("nros_cpp_qos_override_t"));
        assert!(!src.contains("set_qos_overrides"));
    }

    #[test]
    fn typed_emit_rclcpp_shape_constructs_with_handle() {
        // Phase 242.4 (RFC-0044) — an rclcpp-shape component OWNS its node: the
        // entry placement-news it with the executor handle *after* init, then
        // checks ok(); there is no separate `create_node` / `configure`.
        let plan = fixture_plan_rclcpp(&[(
            "ctrl_pkg",
            "controller",
            "controller",
            "ctrl_pkg::Controller",
            "ctrl_pkg/Controller.hpp",
        )]);
        let src = emit_typed(&plan).expect("rclcpp emit ok");
        // construct-with-handle headers + arena slot.
        //
        // phase-427 W4 — this asserted `#include <nros/component_node.hpp>`
        // until the merge deleted that header. The assertion had to MOVE
        // rather than go: it exists to prove the rclcpp-shape branch emits the
        // placement-new header the arena slot needs, and a test that keeps
        // asserting a string the template still emits over a file that no
        // longer exists stays GREEN while every generated entry fails to
        // compile. Both directions are pinned here.
        assert!(src.contains("#include <new> // placement-new into the component arena slot"));
        assert!(src.contains("#include <nros/nros.hpp>"));
        assert!(
            !src.contains("component_node.hpp"),
            "the rclcpp entry must not include the DELETED component_node.hpp"
        );
        assert!(src.contains("#include \"ctrl_pkg/Controller.hpp\""));
        assert!(src.contains(
            "alignas(::ctrl_pkg::Controller) static unsigned char __nros_comp_buf_0[sizeof(::ctrl_pkg::Controller)];"
        ));
        assert!(src.contains("static ::ctrl_pkg::Controller* __nros_comp_0 = nullptr;"));
        // setup: handle → placement-new → ok() check naming the node.
        //
        // Issue 1456 — the handle carries the LAUNCH-DECLARED identity. This
        // fixture's node declares a name (`"controller"`) and no namespace, so
        // the second slot is that name and the third is `nullptr`. The
        // negative direction — a node declaring neither, whose class literal
        // must stand — is `typed_emit_rclcpp_undeclared_identity_is_nullptr`
        // below, and both are in the `cpp_native_shapes` golden.
        assert!(
            src.contains(
                "::nros::NodeHandle __h(::nros::global_handle(), \"controller\", nullptr);"
            )
        );
        assert!(
            src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Controller(__h);")
        );
        assert!(src.contains("if (!__nros_comp_0->ok()) {"));
        assert!(src.contains("report_component_failure(\"controller\""));
        // The rclcpp shape does NOT default-construct a Node or call configure.
        assert!(!src.contains("static ::rclcpp::Node __nros_node_0;"));
        assert!(!src.contains("__nros_comp_0.configure"));
        assert!(!src.contains("create_node(__nros_node_0"));
        // still routes to the real executor via the named overload (phase 266)
        assert!(src.contains(
            "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
        ));
    }

    /// Issue 1456 — the launch identity reaches an `rclcpp`-shape component
    /// through its `NodeHandle`, and BOTH directions are load-bearing.
    ///
    /// Measured before the fix, on a real bus (zenoh router, `ros2 node list
    /// --no-daemon`), for a plan declaring `name="alpha" namespace="/island"`:
    /// the component answered `/rclcpp_class_name` — the literal its class
    /// writes — and its relative topics resolved at the ROOT, beside a
    /// `configure`-shape node in the SAME image answering `/island/beta`.
    ///
    /// The negative direction matters just as much and is the one a resolved
    /// view would get wrong: a node the launch file gives no `name=` must keep
    /// its class's literal, so the handle must read `nullptr` there — NOT the
    /// `exec` that `n.name`'s resolution falls back to, and never `""`.
    #[test]
    fn typed_emit_rclcpp_launch_identity_rides_the_handle() {
        let mut plan = fixture_plan_rclcpp(&[(
            "ctrl_pkg",
            "controller",
            "controller",
            "ctrl_pkg::Controller",
            "ctrl_pkg/Controller.hpp",
        )]);
        plan.nodes[0].name = Some("alpha".into());
        plan.nodes[0].namespace = Some("/island".into());
        let src = emit_typed(&plan).expect("rclcpp emit ok");
        assert!(
            src.contains(
                "::nros::NodeHandle __h(::nros::global_handle(), \"alpha\", \"/island\");"
            ),
            "a launch-declared name and namespace must reach the component's handle;\n{src}"
        );
    }

    #[test]
    fn typed_emit_rclcpp_undeclared_identity_is_nullptr() {
        let mut plan = fixture_plan_rclcpp(&[(
            "ctrl_pkg",
            "controller",
            "controller",
            "ctrl_pkg::Controller",
            "ctrl_pkg/Controller.hpp",
        )]);
        plan.nodes[0].name = None;
        plan.nodes[0].namespace = None;
        let src = emit_typed(&plan).expect("rclcpp emit ok");
        assert!(
            src.contains("::nros::NodeHandle __h(::nros::global_handle(), nullptr, nullptr);"),
            "a node the launch file did not name must hand its component NO identity, so \
             the class's own literal stands;\n{src}"
        );
        assert!(
            !src.contains("__h(::nros::global_handle(), \"controller\""),
            "`controller` is the EXEC, not a declared name — passing it would silently \
             outrank the component class's literal for every node in every launch file \
             that omits `name=`"
        );
        // Narrowed to the handle LINE: the boot-config blob legitimately holds
        // `""` for an unset locator/rmw, so a whole-TU search for it proves
        // nothing.
        let handle_line = src
            .lines()
            .find(|l| l.contains("::nros::NodeHandle __h("))
            .expect("the rclcpp arm emits a handle");
        assert!(
            !handle_line.contains("\"\""),
            "an empty string must never reach the handle: it is `unset` at this edge, \
             and a node named `\"\"` is not a node; got: {handle_line}"
        );
    }

    /// Issue 1456, the other half of the negative direction — a launch file
    /// that writes `name=""` / `namespace=""` says "unset", not "a node with
    /// the empty name". The resolution is in the EMITTER (`filter`), so it
    /// needs its own row: the `None` case above would pass with a `map`.
    #[test]
    fn typed_emit_rclcpp_empty_launch_identity_is_unset() {
        let mut plan = fixture_plan_rclcpp(&[(
            "ctrl_pkg",
            "controller",
            "controller",
            "ctrl_pkg::Controller",
            "ctrl_pkg/Controller.hpp",
        )]);
        plan.nodes[0].name = Some(String::new());
        plan.nodes[0].namespace = Some(String::new());
        let src = emit_typed(&plan).expect("rclcpp emit ok");
        assert!(
            src.contains("::nros::NodeHandle __h(::nros::global_handle(), nullptr, nullptr);"),
            "an empty declared name or namespace is `unset`, so the handle carries \
             nothing and the component class's literal stands;\n{src}"
        );
    }

    /// Issue 1456 — the TIERED arm builds its handle from the tier's executor,
    /// and it needs the identity for the same reason the single-executor arm
    /// does. One `if` in the template selects the executor expression, so a
    /// fix applied to one arm and not the other is exactly the shape this
    /// pins shut.
    #[test]
    fn typed_emit_rclcpp_launch_identity_reaches_the_tiered_arm() {
        // The node keeps its declared NAME (`ctrl`) because a tier's members
        // are matched by name; the namespace is what moves.
        let mut plan = fixture_plan_with_tiers();
        plan.board = "freertos".into();
        for n in &mut plan.nodes {
            n.shape = Some("rclcpp".into());
        }
        plan.nodes[0].namespace = Some("/island".into());
        let src = emit_typed(&plan).expect("tiered rclcpp emit ok");
        assert!(
            src.contains("::nros::NodeHandle __h(executor, \"ctrl\", \"/island\");"),
            "the tiered arm must carry the launch identity too;\n{src}"
        );
        assert!(
            src.contains("::nros::NodeHandle __h(executor, \"telem\", nullptr);"),
            "and the sibling that declared no namespace must still get nullptr there;\n{src}"
        );
    }

    #[test]
    fn typed_emit_mixed_rclcpp_and_configure_shapes() {
        // One rclcpp node + one configure node in the same entry: each constructs
        // its own way; the includes carry both seams.
        let mut plan = fixture_plan_typed(&[
            (
                "ctrl_pkg",
                "controller",
                "controller",
                "ctrl_pkg::Controller",
                "ctrl_pkg/Controller.hpp",
            ),
            (
                "legacy_pkg",
                "legacy",
                "legacy",
                "legacy_pkg::Legacy",
                "legacy_pkg/Legacy.hpp",
            ),
        ]);
        plan.nodes[0].shape = Some("rclcpp".into());
        // plan.nodes[1] stays "configure".
        let src = emit_typed(&plan).expect("mixed emit ok");
        // node 0 = rclcpp: arena slot + handle construct, no Node/configure.
        assert!(src.contains("static ::ctrl_pkg::Controller* __nros_comp_0 = nullptr;"));
        assert!(
            src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Controller(__h);")
        );
        assert!(!src.contains("static ::rclcpp::Node __nros_node_0;"));
        // node 1 = configure: Node + configure, no arena slot.
        assert!(src.contains("static ::rclcpp::Node __nros_node_1;"));
        assert!(src.contains("static ::legacy_pkg::Legacy __nros_comp_1;"));
        assert!(src.contains("__nros_comp_1.configure(__nros_node_1)"));
        assert!(!src.contains("__nros_comp_buf_1"));
        // rclcpp placement-new include present because at least one rclcpp
        // node exists (phase-427 W4 — was `component_node.hpp`, deleted).
        assert!(src.contains("#include <new> // placement-new into the component arena slot"));
        assert!(
            !src.contains("component_node.hpp"),
            "the rclcpp entry must not include the DELETED component_node.hpp"
        );
    }

    #[test]
    fn typed_emit_duplicate_pkg_makes_two_instances_one_include() {
        // Two `<node>` rows of the same pkg → two component objects, one include.
        let plan = fixture_plan_typed(&[
            ("twin_pkg", "a", "a", "twin_pkg::Twin", "twin_pkg/Twin.hpp"),
            ("twin_pkg", "b", "b", "twin_pkg::Twin", "twin_pkg/Twin.hpp"),
        ]);
        let src = emit_typed(&plan).expect("typed emit ok");
        assert_eq!(src.matches("#include \"twin_pkg/Twin.hpp\"").count(), 1);
        assert!(src.contains("static ::twin_pkg::Twin __nros_comp_0;"));
        assert!(src.contains("static ::twin_pkg::Twin __nros_comp_1;"));
        assert!(src.contains("::nros::create_node(__nros_node_0, \"a\", \"/\")"));
        assert!(src.contains("::nros::create_node(__nros_node_1, \"b\", \"/\")"));
    }

    #[test]
    fn typed_emit_c_node_uses_factory_configure_seam() {
        // A `lang == "c"` node routes through the C-ABI factory + configure seam
        // (no C++ class, no header include); the entry hands it `ffi_handle()`.
        let mut plan = fixture_plan_typed(&[(
            "sensor_pkg",
            "sensor",
            "sensor",
            "sensor_pkg::Sensor",
            "sensor_pkg/Sensor.hpp",
        )]);
        plan.nodes[0].lang = Some("c".into());
        let src = emit_typed(&plan).expect("typed emit ok");
        // extern "C" factory + configure decls, mangled on pkg.
        assert!(src.contains("void* __nros_c_component_sensor_pkg_create(void);"));
        assert!(src.contains(
            "int32_t __nros_c_component_sensor_pkg_configure(const ::nros_cpp_node_t* node, void* executor, void* self);"
        ));
        // setup uses create() + configure(ffi_handle, executor_handle, self) — not a C++ class.
        assert!(src.contains("void* self = __nros_c_component_sensor_pkg_create();"));
        assert!(src.contains(
            "__nros_c_component_sensor_pkg_configure(__nros_node_0.ffi_handle(), __nros_node_0.executor_handle(), self)"
        ));
        // No C++ class storage / header / .configure for the C node.
        assert!(!src.contains("static ::sensor_pkg::Sensor"));
        assert!(!src.contains("#include \"sensor_pkg/Sensor.hpp\""));
        assert!(!src.contains("__nros_comp_0.configure"));
        // Still routes to the real executor via the named overload (phase 266).
        assert!(src.contains(
            "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
        ));
    }

    #[test]
    fn typed_emit_mixed_c_and_cpp_nodes() {
        let mut plan = fixture_plan_typed(&[
            (
                "talker_pkg",
                "talker",
                "talker",
                "talker_pkg::Talker",
                "talker_pkg/Talker.hpp",
            ),
            (
                "sensor_pkg",
                "sensor",
                "sensor",
                "sensor_pkg::Sensor",
                "sensor_pkg/Sensor.hpp",
            ),
        ]);
        plan.nodes[1].lang = Some("c".into()); // sensor is C
        let src = emit_typed(&plan).expect("typed emit ok");
        // C++ node: header + class + .configure.
        assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
        assert!(src.contains("static ::talker_pkg::Talker __nros_comp_0;"));
        assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
        // C node: factory seam, no header/class.
        assert!(src.contains("void* self = __nros_c_component_sensor_pkg_create();"));
        assert!(!src.contains("static ::sensor_pkg::Sensor"));
    }

    #[test]
    fn typed_emit_nuttx_board_uses_nuttxboard_run_components() {
        // Phase 266: embedded boards use the 3-arg (locator, session_name, setup) overload.
        let mut plan = fixture_plan_typed(&[("t_pkg", "t", "t", "t_pkg::T", "t_pkg/T.hpp")]);
        plan.board = "nuttx".into();
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(src.contains(
            "::nros::board::NuttxBoard::run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
        ));
    }

    #[test]
    fn typed_emit_threadx_board_uses_threadxboard_run_components() {
        // Phase 246 — the ThreadX family keys (host sim + bare-metal riscv64) all
        // route the typed entry to the `ThreadxBoard` adapter's `run_components`.
        // Phase 266: uses the 3-arg (locator, session_name, setup) named overload.
        for key in [
            "threadx",
            "threadx-linux",
            "threadx-qemu-riscv64",
            "rv-virt-threadx",
        ] {
            let mut plan = fixture_plan_typed(&[("t_pkg", "t", "t", "t_pkg::T", "t_pkg/T.hpp")]);
            plan.board = key.into();
            let src = emit_typed(&plan).expect("typed emit ok");
            assert!(
                src.contains(
                    "::nros::board::ThreadxBoard::run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
                ),
                "board key {key} must map to ThreadxBoard::run_components with named overload"
            );
        }
    }

    #[test]
    fn typed_emit_native_single_node_bakes_name_in_boot_config() {
        // Phase 266 — single-node native entry: boot config carries the node name.
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(src.contains("#include <nros/boot_config.h>"));
        assert!(src.contains("NROS_BOOT_SET_NODE_NAME"));
        assert!(src.contains(".node_name  = \"talker\""));
        assert!(src.contains(
            "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
        ));
    }

    #[test]
    fn typed_emit_errors_when_class_missing() {
        let plan = fixture_plan(&[("talker_pkg", "talker")]); // class_name None
        let err = emit_typed(&plan).unwrap_err();
        assert!(err.contains("missing class_name"), "{err}");
        assert!(err.contains("talker_pkg"), "{err}");
    }

    #[test]
    fn typed_emit_param_services_block_present_when_enabled() {
        // Phase 269 W1 (amended by issue 0745) — param SEEDING now emits per
        // node BEFORE construction (emit_declare_params, the 0255 remap rule:
        // an rclcpp ctor reads declare_parameter initials immediately);
        // param_services gates only the runtime get/set surface.
        let mut plan = fixture_plan_typed(&[(
            "param_talker_pkg",
            "param_talker",
            "param_talker",
            "param_talker_pkg::ParamTalker",
            "param_talker_pkg/ParamTalker.hpp",
        )]);
        plan.param_services = true;
        plan.nodes[0].params = vec![("publish_period_ms".into(), "250".into())];
        let src = emit_typed(&plan).expect("typed cpp emit ok");
        assert!(src.contains("nros_cpp_register_parameter_services(__exec)"));
        assert!(src.contains(
            "nros_cpp_declare_param(::nros::global_handle(), 0, \"publish_period_ms\", \"250\")"
        ));
        // issue 0745 — seeding precedes construction.
        let seed_at = src.find("nros_cpp_declare_param").unwrap();
        let construct_at = src
            .find(".configure(")
            .or_else(|| src.find("new ("))
            .unwrap();
        assert!(
            seed_at < construct_at,
            "param seeding must precede construction"
        );
        // must appear after configure, before return 0
        let reg_at = src.find("nros_cpp_register_parameter_services").unwrap();
        let ret_at = src.rfind("return 0;").unwrap();
        assert!(reg_at < ret_at, "param block must precede return 0");
        // confirms executor handle fetched from global
        assert!(src.contains("::nros::global_handle()"));
    }

    #[test]
    fn typed_emit_param_services_absent_when_disabled() {
        // Guard: non-param plans produce byte-identical output (no param block).
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let src = emit_typed(&plan).expect("typed cpp emit ok");
        assert!(!src.contains("nros_cpp_register_parameter_services"));
        assert!(!src.contains("nros_cpp_declare_param"));
    }

    #[test]
    fn typed_emit_lifecycle_active_emits_autostart_block() {
        // Phase 269 W2 — lifecycle = Some("active") → nros_cpp_lifecycle_autostart(__exec, 2u)
        // in the post-configure block, AFTER any param block, BEFORE return 0.
        let mut plan = fixture_plan_typed(&[(
            "lifecycle_talker_pkg",
            "lifecycle_talker",
            "lifecycle_talker",
            "lifecycle_talker_pkg::LifecycleTalker",
            "lifecycle_talker_pkg/LifecycleTalker.hpp",
        )]);
        plan.lifecycle = Some("active".into());
        let src = emit_typed(&plan).expect("typed cpp lifecycle emit ok");
        // autostart call with code 2 (active = configure + activate)
        assert!(
            src.contains("nros_cpp_lifecycle_autostart(__exec, 2u)"),
            "expected nros_cpp_lifecycle_autostart(__exec, 2u) in:\n{src}"
        );
        // executor handle from global_handle
        assert!(src.contains("::nros::global_handle()"));
        // AFTER configure loop (configure call or C factory), BEFORE return 0
        let autostart_at = src.find("nros_cpp_lifecycle_autostart").unwrap();
        let ret_at = src.rfind("return 0;").unwrap();
        assert!(
            autostart_at < ret_at,
            "lifecycle block must precede return 0"
        );
        // configure call precedes the lifecycle block
        let cfg_at = src.find("__nros_comp_0.configure(__nros_node_0)").unwrap();
        assert!(
            cfg_at < autostart_at,
            "lifecycle block must follow configure call"
        );
    }

    #[test]
    fn typed_emit_lifecycle_configure_emits_code_1() {
        let mut plan = fixture_plan_typed(&[("lc_pkg", "lc", "lc", "lc_pkg::Lc", "lc_pkg/Lc.hpp")]);
        plan.lifecycle = Some("configure".into());
        let src = emit_typed(&plan).expect("typed cpp lifecycle configure emit ok");
        assert!(
            src.contains("nros_cpp_lifecycle_autostart(__exec, 1u)"),
            "expected autostart_code 1 for 'configure'; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_lifecycle_none_emits_code_0() {
        let mut plan = fixture_plan_typed(&[("lc_pkg", "lc", "lc", "lc_pkg::Lc", "lc_pkg/Lc.hpp")]);
        plan.lifecycle = Some("none".into());
        let src = emit_typed(&plan).expect("typed cpp lifecycle none emit ok");
        assert!(
            src.contains("nros_cpp_lifecycle_autostart(__exec, 0u)"),
            "expected autostart_code 0 for 'none'; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_lifecycle_absent_when_disabled() {
        // Guard: lifecycle = None → byte-identical output (no lifecycle block).
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let src = emit_typed(&plan).expect("typed cpp emit ok");
        assert!(
            !src.contains("nros_cpp_lifecycle_autostart"),
            "lifecycle block must be absent when lifecycle = None"
        );
    }

    #[test]
    fn typed_emit_lifecycle_after_param_block() {
        // Phase 269 W2 — when both param_services and lifecycle are set, the lifecycle
        // block must appear AFTER the param block (same order as the Rust macro: params → lifecycle).
        let mut plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        plan.param_services = true;
        plan.nodes[0].params = vec![("foo".into(), "bar".into())];
        plan.lifecycle = Some("active".into());
        let src = emit_typed(&plan).expect("typed cpp combined emit ok");
        let param_at = src.find("nros_cpp_register_parameter_services").unwrap();
        let lc_at = src.find("nros_cpp_lifecycle_autostart").unwrap();
        assert!(
            param_at < lc_at,
            "lifecycle block must follow param-services block"
        );
    }

    // -------------------------------------------------------------------------
    // Phase 269 (W4) — sched-context wiring tests
    // -------------------------------------------------------------------------

    fn fixture_plan_with_tiers() -> Plan {
        use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
        let high_tier = ResolvedTier {
            name: "high".into(),
            priority: 80,
            stack_bytes: None,
            spin_period_us: Some(10_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![("ctrl".into(), "ctrl_grp".into())],
        };
        let low_tier = ResolvedTier {
            name: "low".into(),
            priority: 10,
            stack_bytes: None,
            spin_period_us: Some(100_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![("telem".into(), "telem_grp".into())],
        };
        let mut plan = fixture_plan_typed(&[
            (
                "ctrl_pkg",
                "ctrl",
                "ctrl",
                "ctrl_pkg::Ctrl",
                "ctrl_pkg/Ctrl.hpp",
            ),
            (
                "telem_pkg",
                "telem",
                "telem",
                "telem_pkg::Telem",
                "telem_pkg/Telem.hpp",
            ),
        ]);
        plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
        plan.nodes[0].sched_context = Some(0);
        plan.nodes[1].callback_groups = vec!["telem_grp".into()];
        plan.nodes[1].sched_context = Some(1);
        plan.resolved_tiers = Some(ResolvedTierTable {
            tiers: vec![high_tier, low_tier],
        });
        plan
    }

    /// issue 1272 -- two nodes that set the SAME parameter name each seed it on
    /// their own node index, and each node's seeds come right before that
    /// node's construction, so the index the seed names is the one the
    /// executor hands out next.
    #[test]
    fn typed_emit_seeds_each_node_on_its_own_index() {
        let mut plan = fixture_plan_typed(&[
            ("a_pkg", "alpha", "alpha", "a_pkg::Alpha", "a_pkg/Alpha.hpp"),
            ("b_pkg", "beta", "beta", "b_pkg::Beta", "b_pkg/Beta.hpp"),
        ]);
        plan.nodes[0].params = vec![("rate".into(), "10".into())];
        plan.nodes[1].params = vec![("rate".into(), "20".into())];
        let src = emit_typed(&plan).expect("typed cpp emit ok");

        let seed_a = "nros_cpp_declare_param(::nros::global_handle(), 0, \"rate\", \"10\")";
        let seed_b = "nros_cpp_declare_param(::nros::global_handle(), 1, \"rate\", \"20\")";
        let create_a = "::nros::create_node(__nros_node_0, \"alpha\", \"/\")";
        let create_b = "::nros::create_node(__nros_node_1, \"beta\", \"/\")";
        let at = |s: &str| {
            src.find(s)
                .unwrap_or_else(|| panic!("missing `{s}`; got:\n{src}"))
        };
        assert!(
            at(seed_a) < at(create_a) && at(create_a) < at(seed_b) && at(seed_b) < at(create_b),
            "each node's seeds must directly precede its own construction; got:\n{src}"
        );
    }

    /// issue 1272 -- each tier setup builds its nodes on the tier's OWN
    /// executor, so the first node of every tier is index 0 there.
    #[test]
    fn typed_emit_tier_seeds_restart_at_zero_per_tier() {
        let mut plan = fixture_plan_with_tiers();
        plan.nodes[0].params = vec![("period".into(), "10".into())];
        plan.nodes[1].params = vec![("period".into(), "100".into())];
        let src = emit_typed(&plan).expect("typed cpp tier emit ok");

        assert!(
            src.contains("nros_cpp_declare_param(executor, 0, \"period\", \"10\")"),
            "ctrl is tier 0's first node; got:\n{src}"
        );
        assert!(
            src.contains("nros_cpp_declare_param(executor, 0, \"period\", \"100\")"),
            "telem is tier 1's first node, index 0 on tier 1's executor; got:\n{src}"
        );
        assert!(
            !src.contains("nros_cpp_declare_param(executor, 1,"),
            "no tier builds a second node here; got:\n{src}"
        );
    }

    #[test]
    fn typed_emit_group_split_node_falls_back_to_sched_context_path() {
        // Phase 282 follow-up (RFC-0047) — ONE node with callback groups on TWO
        // tiers (`group_tiers = { ctrl = "high", telem = "low" }`) cannot use
        // run_tiers: per-tier setup fns construct whole nodes, so the node
        // landed on the last tier and both timers ran at that cadence
        // (regression caught by realtime_subnode_cpp_e2e: ctrl=6 telem=5).
        // Such plans must keep the single-executor sched-context path.
        use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
        let high_tier = ResolvedTier {
            name: "high".into(),
            priority: 80,
            stack_bytes: None,
            spin_period_us: Some(10_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![("sub_node".into(), "ctrl".into())],
        };
        let low_tier = ResolvedTier {
            name: "low".into(),
            priority: 10,
            stack_bytes: None,
            spin_period_us: Some(100_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![("sub_node".into(), "telem".into())],
        };
        let mut plan = fixture_plan_typed(&[(
            "subnode_pkg",
            "sub_node",
            "sub_node",
            "subnode_pkg::SubNode",
            "subnode_pkg/SubNode.hpp",
        )]);
        plan.nodes[0].callback_groups = vec!["ctrl".into(), "telem".into()];
        plan.resolved_tiers = Some(ResolvedTierTable {
            tiers: vec![high_tier, low_tier],
        });
        let src = emit_typed(&plan).expect("typed cpp group-split emit ok");

        // Sched-context path: per-group seeding present, run_tiers absent.
        assert!(
            src.contains("nros_cpp_bind_group_sched"),
            "group-split node must seed bind_group_sched; src:\n{src}"
        );
        assert!(
            src.contains("\"ctrl\"") && src.contains("\"telem\""),
            "both groups must be seeded; src:\n{src}"
        );
        assert!(
            !src.contains("__nros_entry_setup_tier_0"),
            "group-split plan must NOT use the run_tiers path; src:\n{src}"
        );
        assert!(
            !src.contains("run_tiers("),
            "group-split plan must NOT call run_tiers; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_tiers_native_uses_run_tiers_path() {
        // Phase 274.W2 — native board + multi-tier emits per-tier setup functions +
        // run_tiers call instead of the old sched-context wiring.
        let plan = fixture_plan_with_tiers();
        let src = emit_typed(&plan).expect("typed cpp tier emit ok");

        // Per-tier setup functions emitted.
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
            "expected tier-0 setup fn; got:\n{src}"
        );
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
            "expected tier-1 setup fn; got:\n{src}"
        );
        // Each setup fn creates only its tier's nodes via create_node_on.
        assert!(
            src.contains("::nros::create_node_on(__nros_node_0, executor, \"ctrl\", \"/\")"),
            "ctrl node must use create_node_on in tier-0 setup; src:\n{src}"
        );
        assert!(
            src.contains("::nros::create_node_on(__nros_node_1, executor, \"telem\", \"/\")"),
            "telem node must use create_node_on in tier-1 setup; src:\n{src}"
        );
        // NativeTierSpec array emitted.
        assert!(
            src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
            "expected 2-element NativeTierSpec array; src:\n{src}"
        );
        assert!(
            src.contains("\"high\""),
            "high tier name in spec table; src:\n{src}"
        );
        assert!(
            src.contains("\"low\""),
            "low tier name in spec table; src:\n{src}"
        );
        assert!(src.contains("80LL"), "high priority 80LL; src:\n{src}");
        assert!(src.contains("10LL"), "low priority 10LL; src:\n{src}");
        // main calls run_tiers.
        assert!(
            src.contains("::nros::board::LinuxBoard::run_tiers("),
            "main must call LinuxBoard::run_tiers; src:\n{src}"
        );
        // Old sched-context wiring must NOT appear in the run_tiers path.
        assert!(
            !src.contains("__nros_sc_ids"),
            "run_tiers path must not emit sc_ids; src:\n{src}"
        );
        assert!(
            !src.contains("nros_cpp_create_sched_context"),
            "run_tiers path must not emit create_sched_context; src:\n{src}"
        );
        assert!(
            !src.contains("nros_cpp_bind_node_name_sched"),
            "run_tiers path must not emit bind_node_name_sched; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_tiers_embedded_uses_sched_context_path() {
        // Phase 272/273 (W2) — a sched-context embedded board (ThreadX) + multi-tier
        // still uses sched-context wiring (bind_node_name_sched + bind_group_sched) because
        // run_tiers is limited to native + FreeRTOS + Zephyr + NuttX (phase-281 W3/W3a).
        // ThreadX keeps board_is_embedded=true && !run_tiers → the single-executor
        // sched-context path.
        use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
        let high_tier = ResolvedTier {
            name: "high".into(),
            priority: 80,
            stack_bytes: None,
            spin_period_us: Some(10_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![("ctrl".into(), "ctrl_grp".into())],
        };
        let low_tier = ResolvedTier {
            name: "low".into(),
            priority: 10,
            stack_bytes: None,
            spin_period_us: Some(100_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![("telem".into(), "telem_grp".into())],
        };
        let mut plan = fixture_plan_typed(&[
            (
                "ctrl_pkg",
                "ctrl",
                "ctrl",
                "ctrl_pkg::Ctrl",
                "ctrl_pkg/Ctrl.hpp",
            ),
            (
                "telem_pkg",
                "telem",
                "telem",
                "telem_pkg::Telem",
                "telem_pkg/Telem.hpp",
            ),
        ]);
        // Sched-context embedded board (ThreadX) → sched-context path (NOT run_tiers).
        plan.board = "threadx".into();
        plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
        plan.nodes[0].sched_context = Some(0);
        plan.nodes[1].callback_groups = vec!["telem_grp".into()];
        plan.nodes[1].sched_context = Some(1);
        plan.resolved_tiers = Some(ResolvedTierTable {
            tiers: vec![high_tier, low_tier],
        });
        let src = emit_typed(&plan).expect("typed cpp embedded tier emit ok");
        // Sched-context IDs array declared.
        assert!(
            src.contains("uint8_t __nros_sc_ids[2] = {0};"),
            "embedded tier must emit sc_ids array; got:\n{src}"
        );
        // High tier: no RT class → Fifo SC via the common-backend call, carrying
        // only os_pri=80 (nullptr/0 = absent policy). RFC-0052: the codegen
        // forwards RAW tier fields; the lowering lives in the FFI backend.
        assert!(
            src.contains(
                "nros_cpp_create_sched_context_from_policy(__exec, nullptr, 0ull, 0ull, 0ull, nullptr, 80u, &__nros_sc_ids[0])"
            ),
            "expected tier 0 from_policy call (Fifo, os_pri=80); got:\n{src}"
        );
        // Bind seeds for each tiered node.
        assert!(
            src.contains(
                "nros_cpp_bind_node_name_sched(__exec, \"ctrl\", \"/\", __nros_sc_ids[0])"
            ),
            "ctrl must be seeded; src:\n{src}"
        );
        assert!(
            src.contains(
                "nros_cpp_bind_node_name_sched(__exec, \"telem\", \"/\", __nros_sc_ids[1])"
            ),
            "telem must be seeded; src:\n{src}"
        );
        // run_tiers must NOT be called (embedded boards use single executor).
        assert!(
            !src.contains("LinuxBoard::run_tiers"),
            "embedded board must not emit run_tiers; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_single_executor_forwards_real_time_tier_to_backend() {
        // Phase 297 W1 / RFC-0052 (common backend) — the single-executor
        // sched-context path (ThreadX + group-split) forwards a `real_time`
        // tier's RAW class/budget/period/deadline to
        // `nros_cpp_create_sched_context_from_policy`, whose backend
        // (`SchedContext::from_tier_policy`) does the class→Sporadic lowering —
        // the SAME one the Rust runtime uses. The codegen re-derives nothing.
        use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
        let rt_tier = ResolvedTier {
            name: "control".into(),
            priority: 90,
            stack_bytes: None,
            spin_period_us: Some(5_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: Some("real_time".into()),
            period_us: Some(20_000),
            budget_us: Some(3_000),
            deadline_us: Some(15_000),
            deadline_policy: Some("fault".into()),
            core: None,
            members: vec![("ctrl".into(), "ctrl_grp".into())],
        };
        let mut plan = fixture_plan_typed(&[(
            "ctrl_pkg",
            "ctrl",
            "ctrl",
            "ctrl_pkg::Ctrl",
            "ctrl_pkg/Ctrl.hpp",
        )]);
        plan.board = "threadx".into();
        plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
        plan.nodes[0].sched_context = Some(0);
        plan.resolved_tiers = Some(ResolvedTierTable {
            tiers: vec![rt_tier],
        });
        let src = emit_typed(&plan).expect("typed cpp real_time tier emit ok");
        // RFC-0052 common backend: the codegen forwards the RAW tier fields to
        // `nros_cpp_create_sched_context_from_policy`; the class→Sporadic +
        // budget/period lowering happens in the FFI backend
        // (`SchedContext::from_tier_policy`), unit-tested in nros-node. The
        // codegen must NOT re-derive the mapping (no `__sc.class_ = ...`).
        assert!(
            src.contains(
                "nros_cpp_create_sched_context_from_policy(__exec, \"real_time\", 20000ull, 3000ull, 15000ull, \"fault\", 90u, &__nros_sc_ids[0])"
            ),
            "real_time tier must forward raw fields to the backend; got:\n{src}"
        );
        assert!(
            !src.contains("__sc.class_"),
            "codegen must not re-derive the class mapping (common backend); got:\n{src}"
        );
    }

    #[test]
    fn typed_emit_tiers_freertos_embedded_uses_run_tiers_path() {
        // Phase 274.W3 — FreeRTOS embedded board + multi-tier emits per-tier setup
        // functions + FreertosBoard::run_tiers via nros_app_main +
        // NROS_APP_MAIN_REGISTER_VOID (NOT the sched-context path, NOT int main).
        let mut plan = fixture_plan_with_tiers();
        plan.board = "freertos".into(); // FreertosBoard

        let src = emit_typed(&plan).expect("typed cpp freertos tier emit ok");

        // Per-tier setup functions emitted.
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
            "expected tier-0 setup fn; got:\n{src}"
        );
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
            "expected tier-1 setup fn; got:\n{src}"
        );
        // NativeTierSpec array emitted.
        assert!(
            src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
            "expected 2-element NativeTierSpec array; src:\n{src}"
        );
        // FreertosBoard::run_tiers called (not LinuxBoard).
        assert!(
            src.contains("::nros::board::FreertosBoard::run_tiers("),
            "nros_app_main must call FreertosBoard::run_tiers; src:\n{src}"
        );
        // FreeRTOS embedded entry point: nros_app_main + NROS_APP_MAIN_REGISTER_VOID.
        assert!(
            src.contains("extern \"C\" int nros_app_main("),
            "FreeRTOS run_tiers must emit nros_app_main; src:\n{src}"
        );
        assert!(
            src.contains("NROS_APP_MAIN_REGISTER_VOID()"),
            "FreeRTOS run_tiers must emit NROS_APP_MAIN_REGISTER_VOID; src:\n{src}"
        );
        // NOT int main (that's native).
        assert!(
            !src.contains("int main("),
            "FreeRTOS run_tiers must NOT emit int main; src:\n{src}"
        );
        // Old sched-context wiring must NOT appear.
        assert!(
            !src.contains("__nros_sc_ids"),
            "FreeRTOS run_tiers path must not emit sc_ids; src:\n{src}"
        );
        assert!(
            !src.contains("nros_cpp_create_sched_context"),
            "FreeRTOS run_tiers path must not emit create_sched_context; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_tiers_zephyr_embedded_uses_run_tiers_path() {
        // phase-281 W3a — Zephyr embedded board + multi-tier emits per-tier setup
        // functions + ZephyrBoard::run_tiers via a plain `int main(void)` (the Zephyr
        // kernel calls main directly — NO nros_app_main, NO sched-context path).
        let mut plan = fixture_plan_with_tiers();
        plan.board = "zephyr".into(); // ZephyrBoard

        let src = emit_typed(&plan).expect("typed cpp zephyr tier emit ok");

        // Per-tier setup functions emitted.
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
            "expected tier-0 setup fn; got:\n{src}"
        );
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
            "expected tier-1 setup fn; got:\n{src}"
        );
        // NativeTierSpec array emitted.
        assert!(
            src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
            "expected 2-element NativeTierSpec array; src:\n{src}"
        );
        // ZephyrBoard::run_tiers called (not LinuxBoard / FreertosBoard).
        assert!(
            src.contains("::nros::board::ZephyrBoard::run_tiers("),
            "main must call ZephyrBoard::run_tiers; src:\n{src}"
        );
        // Zephyr entry point: plain int main(void), kernel calls it directly.
        assert!(
            src.contains("int main(void) {"),
            "Zephyr run_tiers must emit int main(void); src:\n{src}"
        );
        // NOT the FreeRTOS/startup.c app_main shape.
        assert!(
            !src.contains("nros_app_main"),
            "Zephyr run_tiers must NOT emit nros_app_main; src:\n{src}"
        );
        assert!(
            !src.contains("NROS_APP_MAIN_REGISTER_VOID"),
            "Zephyr run_tiers must NOT emit NROS_APP_MAIN_REGISTER_VOID; src:\n{src}"
        );
        // Old sched-context wiring must NOT appear (this is the run_tiers path).
        assert!(
            !src.contains("__nros_sc_ids"),
            "Zephyr run_tiers path must not emit sc_ids; src:\n{src}"
        );
        assert!(
            !src.contains("nros_cpp_create_sched_context"),
            "Zephyr run_tiers path must not emit create_sched_context; src:\n{src}"
        );
        // run_tiers path must not CALL run_components (the string appears once in the
        // file-header doc comment, so assert on the call form specifically).
        assert!(
            !src.contains("ZephyrBoard::run_components"),
            "Zephyr run_tiers path must not call ZephyrBoard::run_components; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_tiers_nuttx_embedded_uses_run_tiers_path() {
        // phase-281 W3 (nuttx) — NuttX embedded board + multi-tier emits per-tier
        // setup functions + NuttxBoard::run_tiers via nros_app_main +
        // NROS_APP_MAIN_REGISTER_VOID (the NuttX startup path calls app_main, like
        // FreeRTOS — NOT Zephyr's int main(void), NOT the sched-context path).
        let mut plan = fixture_plan_with_tiers();
        plan.board = "nuttx".into(); // NuttxBoard

        let src = emit_typed(&plan).expect("typed cpp nuttx tier emit ok");

        // Per-tier setup functions emitted.
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
            "expected tier-0 setup fn; got:\n{src}"
        );
        assert!(
            src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
            "expected tier-1 setup fn; got:\n{src}"
        );
        // NativeTierSpec array emitted.
        assert!(
            src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
            "expected 2-element NativeTierSpec array; src:\n{src}"
        );
        // NuttxBoard::run_tiers called (not LinuxBoard / FreertosBoard / ZephyrBoard).
        assert!(
            src.contains("::nros::board::NuttxBoard::run_tiers("),
            "nros_app_main must call NuttxBoard::run_tiers; src:\n{src}"
        );
        // NuttX embedded entry point: nros_app_main + NROS_APP_MAIN_REGISTER_VOID
        // (the app_main startup shape, shared with FreeRTOS).
        assert!(
            src.contains("extern \"C\" int nros_app_main("),
            "NuttX run_tiers must emit nros_app_main; src:\n{src}"
        );
        assert!(
            src.contains("NROS_APP_MAIN_REGISTER_VOID()"),
            "NuttX run_tiers must emit NROS_APP_MAIN_REGISTER_VOID; src:\n{src}"
        );
        // NOT int main (that's native) and NOT the Zephyr int main(void).
        assert!(
            !src.contains("int main("),
            "NuttX run_tiers must NOT emit int main; src:\n{src}"
        );
        // Old sched-context wiring must NOT appear (this is the run_tiers path).
        assert!(
            !src.contains("__nros_sc_ids"),
            "NuttX run_tiers path must not emit sc_ids; src:\n{src}"
        );
        assert!(
            !src.contains("nros_cpp_create_sched_context"),
            "NuttX run_tiers path must not emit create_sched_context; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_tiers_rclcpp_embedded_node_is_seeded() {
        // Phase 272 (W2) — rclcpp-shape tiered node on an embedded board IS seeded
        // via bind_node_name_sched (the #124 dissolve). Native boards use run_tiers
        // instead; this test covers the embedded (sched-context) path.
        use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
        let high_tier = ResolvedTier {
            name: "high".into(),
            priority: 80,
            stack_bytes: None,
            spin_period_us: Some(10_000),
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: vec![("ctrl".into(), "ctrl_grp".into())],
        };
        let mut plan = fixture_plan_rclcpp(&[(
            "ctrl_pkg",
            "ctrl",
            "ctrl",
            "ctrl_pkg::Ctrl",
            "ctrl_pkg/Ctrl.hpp",
        )]);
        // ThreadX — a sched-context embedded board (phase-281 W3a moved Zephyr and
        // W3(nuttx) moved NuttX onto the run_tiers path, so this seeding proof now
        // uses a board that still schedules via the single-executor sched-context wiring).
        plan.board = "threadx".into();
        plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
        plan.nodes[0].sched_context = Some(0);
        plan.resolved_tiers = Some(ResolvedTierTable {
            tiers: vec![high_tier],
        });
        let src = emit_typed(&plan).expect("rclcpp embedded tier emit ok");
        // rclcpp-shape node MUST be seeded (the #124 proof, embedded path).
        assert!(
            src.contains(
                "nros_cpp_bind_node_name_sched(__exec, \"ctrl\", \"/\", __nros_sc_ids[0])"
            ),
            "rclcpp-shape tiered node must be seeded via bind_node_name_sched; src:\n{src}"
        );
        // rclcpp construction path unchanged (placement-new with handle).
        assert!(
            src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Ctrl(__h);"),
            "rclcpp node still constructs via placement-new"
        );
        // Seed precedes construction.
        let seed_at = src
            .find("nros_cpp_bind_node_name_sched(__exec, \"ctrl\"")
            .unwrap();
        let ctor_at = src.find("new (__nros_comp_buf_0)").unwrap();
        assert!(seed_at < ctor_at, "seed must precede rclcpp construction");
    }

    #[test]
    fn typed_emit_no_tiers_uses_plain_create_node() {
        // Guard: empty resolved_tiers keeps byte-identical plain create (no seed, no sched).
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let src = emit_typed(&plan).expect("typed cpp no-tier emit ok");
        assert!(
            !src.contains("__nros_sc_ids"),
            "no-tier plan must not emit sc_ids"
        );
        assert!(
            !src.contains("nros_cpp_create_sched_context"),
            "no-tier plan must not emit sched_context_create"
        );
        assert!(
            !src.contains("nros_cpp_bind_node_name_sched"),
            "no-tier plan must not emit bind_node_name_sched"
        );
        assert!(
            src.contains("::nros::create_node(__nros_node_0, \"talker\", \"/\")"),
            "no-tier plan must use plain create_node"
        );
        assert!(
            !src.contains(".sched("),
            "no-tier plan must not use NodeBuilder sched"
        );
    }

    /// Issue 1443 — the C++ pack passes the PLAN's namespace to `create_node`
    /// (single executor) and `create_node_on` (a tier's own), instead of
    /// relying on either function's `nullptr` default.
    ///
    /// Same three inputs and one answer as the C pack's twin: a real namespace
    /// renders itself, `None` and `Some("")` render `"/"`, never `""`. Both
    /// packs read `super::node_namespace`, so they cannot answer differently —
    /// which is the cross-language divergence the issue reported.
    #[test]
    fn typed_emit_creates_each_node_at_its_plan_namespace() {
        for (declared, rendered) in [
            (Some("/island"), "/island"),
            (Some("/a/b"), "/a/b"),
            (None, "/"),
            (Some(""), "/"),
        ] {
            let mut plan = fixture_plan_typed(&[(
                "talker_pkg",
                "talker",
                "talker",
                "talker_pkg::Talker",
                "talker_pkg/Talker.hpp",
            )]);
            plan.nodes[0].namespace = declared.map(str::to_string);
            let src = emit_typed(&plan).expect("typed cpp emit ok");
            assert!(
                src.contains(&format!(
                    "::nros::create_node(__nros_node_0, \"talker\", \"{rendered}\")"
                )),
                "namespace {declared:?} must render as {rendered:?}; src:\n{src}"
            );
            assert!(
                !src.contains("::nros::create_node(__nros_node_0, \"talker\", \"\")"),
                "an empty namespace must never reach the C++ edge; src:\n{src}"
            );
        }
    }

    /// The tiered arm of the same rule — `create_node_on`, one tier each.
    #[test]
    fn typed_emit_tiered_creates_each_node_at_its_plan_namespace() {
        let mut plan = fixture_plan_with_tiers();
        plan.nodes[0].namespace = Some("/island".into());
        // nodes[1] keeps `None`, so both arms appear in one render.
        let src = emit_typed(&plan).expect("typed cpp tiered emit ok");
        assert!(
            src.contains("::nros::create_node_on(__nros_node_0, executor, \"ctrl\", \"/island\")"),
            "tier-0 node must be created under its plan namespace; src:\n{src}"
        );
        assert!(
            src.contains("::nros::create_node_on(__nros_node_1, executor, \"telem\", \"/\")"),
            "a node the plan gives no namespace stays at the root; src:\n{src}"
        );
    }

    // ---------------------------------------------------------------
    // Issue 1003 — the boot wrapper has ONE derivation
    // ---------------------------------------------------------------

    /// The wrapper each board family gets. Written as a table because the bug
    /// this replaces was a board missing from one of two hand-written branch
    /// chains: a table makes an omission visible as a missing row.
    #[test]
    fn every_board_family_derives_its_boot_shape_once() {
        for (board, want) in [
            ("native", BootShape::Host),
            ("posix", BootShape::Host),
            ("zephyr", BootShape::Kernel),
            ("nuttx", BootShape::App),
            ("freertos", BootShape::App),
            ("threadx", BootShape::App),
            ("threadx-linux", BootShape::App),
        ] {
            assert_eq!(
                boot_shape(board),
                want,
                "board '{board}' derived the wrong boot shape"
            );
        }
    }

    /// ThreadX's `startup.c` owns `main`, so its entry must be `nros_app_main`
    /// — a host `int main` would be a second `main` in an image whose board
    /// already defines one.
    ///
    /// ThreadX is the one board the two former branch chains disagreed about,
    /// and it is kept out of the per-tier chain by `use_run_tiers`. Pinning it
    /// here means the shared derivation is right about it on its own terms,
    /// rather than by an argument about a condition elsewhere.
    #[test]
    fn threadx_is_not_treated_as_a_host_board() {
        assert_ne!(
            boot_shape("threadx"),
            BootShape::Host,
            "ThreadX boots through the board's startup.c, so a host `int main` \
would collide with the one the board defines"
        );
        assert_eq!(boot_shape("threadx"), boot_shape("nuttx"));
    }

    /// The boards that DO have `run_tiers` still emit it, each in its own
    /// wrapper — the consolidation must not have narrowed what works.
    #[test]
    fn boards_with_run_tiers_still_emit_their_own_wrapper() {
        for (board, wrapper) in [
            ("native", "int main(int /*argc*/, char** /*argv*/) {"),
            ("zephyr", "int main(void) {"),
            (
                "nuttx",
                "extern \"C\" int nros_app_main(int /*argc*/, char** /*argv*/) {",
            ),
            (
                "freertos",
                "extern \"C\" int nros_app_main(int /*argc*/, char** /*argv*/) {",
            ),
        ] {
            let mut plan = fixture_plan_with_tiers();
            plan.board = board.into();
            let src =
                emit_typed(&plan).unwrap_or_else(|e| panic!("{board} multi-tier emit failed: {e}"));
            assert!(
                src.contains("::run_tiers("),
                "{board} must still call run_tiers"
            );
            assert!(
                src.contains(wrapper),
                "{board} must be wrapped in `{wrapper}`"
            );
        }
    }

    /// Only the host board resolves its locator at runtime, so it is the one
    /// that passes none. Both halves of that rule now come from one place.
    #[test]
    fn only_the_host_entry_omits_the_locator_argument() {
        let mut plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);

        plan.board = "native".into();
        let host = emit_typed(&plan).expect("native emit ok");
        assert!(
            host.contains("run_components(nros_boot_config_node_name("),
            "the host entry passes no locator: {host}"
        );

        plan.board = "nuttx".into();
        let embedded = emit_typed(&plan).expect("nuttx emit ok");
        assert!(
            embedded.contains("run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name("),
            "an embedded entry passes NROS_ENTRY_LOCATOR: {embedded}"
        );
    }

    // -----------------------------------------------------------------------
    // phase-462 W1 (RFC-0052) -- the contract monitor table region.
    // -----------------------------------------------------------------------

    use crate::orchestration::model_ingest::{AgeRow, MonitorRow, render_monitor_rs};

    fn monitor_fixture_rows() -> (Vec<MonitorRow>, Vec<AgeRow>) {
        (
            vec![MonitorRow {
                topic: "/chatter".into(),
                fqn: "/talker/chatter".into(),
                min_rate_hz_milli: 10_000,
                max_latency_ms: 30,
            }],
            vec![AgeRow {
                topic: "/chatter".into(),
                fqn: "/listener/chatter".into(),
                max_age_ms: 150,
            }],
        )
    }

    /// The C++ entry bakes the SAME rows the Rust road renders into
    /// `system_monitors.rs`, field for field, and installs them before the
    /// first node exists.
    #[test]
    fn typed_emit_bakes_monitor_table_before_nodes() {
        let plan = fixture_plan_typed(&[
            (
                "talker_pkg",
                "talker",
                "talker",
                "talker_pkg::Talker",
                "talker_pkg/Talker.hpp",
            ),
            (
                "listener_pkg",
                "listener",
                "listener",
                "listener_pkg::Listener",
                "listener_pkg/Listener.hpp",
            ),
        ]);
        let (rows, ages) = monitor_fixture_rows();
        let src = emit_typed_monitored(&plan, &rows, &ages).expect("monitored emit ok");
        let rust = render_monitor_rs(&rows, &ages);

        // Row parity, one row at a time: what the Rust table says, the C++
        // table says, in its own spelling.
        for r in &rows {
            let cpp_row = format!(
                "{{ \"{}\", \"{}\", {}u, {}u }},",
                r.topic, r.fqn, r.min_rate_hz_milli, r.max_latency_ms
            );
            let rust_row = format!(
                "topic: {:?}, fqn: {:?}, min_rate_hz_milli: {}u32, max_latency_ms: {}u32",
                r.topic, r.fqn, r.min_rate_hz_milli, r.max_latency_ms
            );
            assert!(
                src.contains(&cpp_row),
                "C++ row `{cpp_row}` missing; src:\n{src}"
            );
            assert!(
                rust.contains(&rust_row),
                "Rust row `{rust_row}` missing; rs:\n{rust}"
            );
        }
        for a in &ages {
            let cpp_row = format!("{{ \"{}\", \"{}\", {}u }},", a.topic, a.fqn, a.max_age_ms);
            let rust_row = format!(
                "topic: {:?}, fqn: {:?}, max_age_ms: {}u32",
                a.topic, a.fqn, a.max_age_ms
            );
            assert!(
                src.contains(&cpp_row),
                "C++ age row `{cpp_row}` missing; src:\n{src}"
            );
            assert!(
                rust.contains(&rust_row),
                "Rust age row `{rust_row}` missing; rs:\n{rust}"
            );
        }
        assert_eq!(src.matches("__nros_mon_rows[1]").count(), 1, "{src}");
        assert_eq!(src.matches("__nros_age_rows[1]").count(), 1, "{src}");
        assert!(
            src.contains("__nros_mon_storage[1 * NROS_CPP_MONITOR_ROW_STORAGE]"),
            "{src}"
        );
        assert!(
            src.contains(".n_rows = 1u,") && src.contains(".n_ages = 1u,"),
            "{src}"
        );

        // Installed BEFORE entity creation, on the process-global executor.
        let install_at = src
            .find("nros_cpp_install_monitors(")
            .expect("install call");
        let create_at = src.find("::nros::create_node(").expect("create_node");
        assert!(
            install_at < create_at,
            "install must precede create_node; src:\n{src}"
        );
        assert!(
            src.contains("void* __mexec = ::nros::global_handle();"),
            "{src}"
        );
    }

    /// RFC-0052's zero-cost claim, at the source: no rows, no table, no
    /// call -- and the TU is the byte-identical one the unmonitored emitter
    /// produces (the goldens are that emitter's).
    #[test]
    fn typed_emit_no_monitor_rows_is_byte_identical() {
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let plain = emit_typed(&plan).expect("emit ok");
        let monitored = emit_typed_monitored(&plan, &[], &[]).expect("emit ok");
        assert_eq!(plain, monitored);
        assert!(!plain.contains("nros_cpp_install_monitors"), "{plain}");
        assert!(!plain.contains("nros_cpp_monitor_row_t"), "{plain}");
        assert!(!plain.contains("NROS_CPP_MONITOR_ROW_STORAGE"), "{plain}");
    }

    /// The model's rows cover every node in the system; a row whose node this
    /// entry does not construct is not this entry's to watch.
    #[test]
    fn typed_emit_keeps_only_rows_of_nodes_it_constructs() {
        let plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        let (rows, ages) = monitor_fixture_rows();
        // `ages` is `/listener/chatter`; no listener here.
        let src = emit_typed_monitored(&plan, &rows, &ages).expect("emit ok");
        assert!(src.contains("\"/talker/chatter\""), "{src}");
        assert!(!src.contains("\"/listener/chatter\""), "{src}");
        assert!(
            src.contains(".ages = nullptr,") && src.contains(".n_ages = 0u,"),
            "{src}"
        );
        assert!(!src.contains("__nros_age_rows"), "{src}");

        // Nothing of ours at all -> no region.
        let other = vec![MonitorRow {
            topic: "/x".into(),
            fqn: "/elsewhere/x".into(),
            min_rate_hz_milli: 1_000,
            max_latency_ms: 0,
        }];
        let src = emit_typed_monitored(&plan, &other, &[]).expect("emit ok");
        assert_eq!(src, emit_typed(&plan).unwrap());
    }

    /// A namespaced node keys its rows by its full name.
    #[test]
    fn typed_emit_monitor_rows_match_namespaced_nodes() {
        let mut plan =
            fixture_plan_typed(&[("cm_pkg", "pub", "pub", "cm_pkg::Pub", "cm_pkg/Pub.hpp")]);
        plan.nodes[0].namespace = Some("/cm".into());
        let rows = vec![MonitorRow {
            topic: "/cm_header".into(),
            fqn: "/cm/pub/cm_header".into(),
            min_rate_hz_milli: 10_000,
            max_latency_ms: 0,
        }];
        let src = emit_typed_monitored(&plan, &rows, &[]).expect("emit ok");
        assert!(
            src.contains("{ \"/cm_header\", \"/cm/pub/cm_header\", 10000u, 0u },"),
            "{src}"
        );
    }

    /// run_tiers shape: each tier installs ITS nodes' rows on ITS executor,
    /// so a row is checked once. Here `ctrl` is on tier 0 and `telem` on
    /// tier 1.
    #[test]
    fn typed_emit_tiers_slice_monitor_rows_per_tier() {
        let plan = fixture_plan_with_tiers();
        let rows = vec![
            MonitorRow {
                topic: "/cmd".into(),
                fqn: "/ctrl/cmd".into(),
                min_rate_hz_milli: 100_000,
                max_latency_ms: 5,
            },
            MonitorRow {
                topic: "/telemetry".into(),
                fqn: "/telem/telemetry".into(),
                min_rate_hz_milli: 1_000,
                max_latency_ms: 0,
            },
        ];
        let src = emit_typed_monitored(&plan, &rows, &[]).expect("tiered emit ok");
        assert!(
            src.contains("__nros_mon_rows_t0[1]") && src.contains("__nros_mon_rows_t1[1]"),
            "{src}"
        );
        let t0 = src
            .find("static int32_t __nros_entry_setup_tier_0(void* executor)")
            .unwrap();
        let t1 = src
            .find("static int32_t __nros_entry_setup_tier_1(void* executor)")
            .unwrap();
        let setup0 = &src[t0..t1];
        let setup1 = &src[t1..];
        assert!(setup0.contains(".rows = __nros_mon_rows_t0,"), "{setup0}");
        assert!(!setup0.contains("__nros_mon_rows_t1"), "{setup0}");
        assert!(setup1.contains(".rows = __nros_mon_rows_t1,"), "{setup1}");
        assert!(setup1.contains("void* __mexec = executor;"), "{setup1}");
        // Each install precedes that tier's first node.
        let i0 = setup0.find("nros_cpp_install_monitors(").unwrap();
        let c0 = setup0.find("::nros::create_node_on(").unwrap();
        assert!(i0 < c0, "{setup0}");
        // And the whole tier table region is absent when no tier has rows.
        assert!(
            !emit_typed(&plan)
                .unwrap()
                .contains("nros_cpp_install_monitors")
        );
    }
}
