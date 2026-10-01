//! Stage 2 for every entry pack: the CLI's [`Plan`] becomes the
//! [`LoweredEntry`] a pack renders from.
//!
//! phase-474 W2 / RFC-0091 §6b. Until this module the C and C++ emitters each
//! projected the plan into their OWN view structs (`CEntryView` and four
//! siblings, `CppEntryView` and six), and a seventh family of shared views
//! lived in `mod.rs`. Read side by side they were one projection written
//! twice — same tier setups, same per-executor index, same rows, binds, boot
//! config and services — differing only where a language spells something.
//! This is that projection, once. Its output is the TEMPLATE CONTEXT: a pack
//! reads `LoweredEntry` directly, and spells what it reads through its own
//! filters (see `filters.rs`).
//!
//! Two entry points, one per producer family:
//!
//! * [`lower_entry`] — the per-node runtime bake the two RUST producers share
//!   (the proc-macro and the parity renderer). Infallible, and deliberately
//!   blind to the image: a Rust-only board key (`esp32-qemu`) is legal here.
//! * [`lower_image`] — that, plus the IMAGE a C-ABI entry pack needs. Fails on
//!   what the plan cannot express (an unknown board, a C++ component with no
//!   class, a tier field the tier ABI has no slot for, a boot-config string
//!   longer than its C buffer).
//!
//! The types live in `nros-entry-lower` (the crate the proc-macro can afford,
//! issue 0083); this builder lives here because `Plan` does.

use std::collections::HashMap;

use nros_entry_lower::{
    AgeRow, ComponentKind, ComponentSeam, GroupBind, LoweredBoot, LoweredBootConfig, LoweredEntry,
    LoweredNode, LoweredProbe, LoweredRunners, LoweredSched, LoweredServices, LoweredTiers,
    MonitorRow, MonitorTable, NodeBind, NodeIdentity, QosOverride, SchedContext, SetupNode,
    TierRow, TierSetup,
};
use nros_orchestration_ir::ResolvedTierTable;

use super::{ExecutorShape, Lang, Plan, PlanNode};
use crate::orchestration::model_ingest;

/// phase-308 W1 — the identity a metadata probe stamps into the sidecar it
/// writes. Its presence is what makes a lowering a PROBE: the same setup body
/// as the entry, with a recording tail instead of the board wrapper.
pub struct ProbeExport {
    pub package: String,
    pub component: String,
    pub executable: String,
    /// The sidecar's `language` field. phase-469 — the enum; the STRING is
    /// produced once, here, by [`Lang::as_str`], which is `Language`'s serde
    /// repr, so the two cannot drift.
    pub language: Lang,
    /// Absolute path the probe writes the sidecar to.
    pub out_path: String,
}

/// What a caller adds to a plan's own facts.
#[derive(Default)]
pub struct LowerOptions<'a> {
    /// phase-462 W1 — the model's contract monitor rows, sliced here per
    /// executor. Read from the model by the CLI rather than carried on the
    /// `Plan`, which is built in ~40 struct literals across the tree.
    pub monitors: &'a [model_ingest::MonitorRow],
    pub ages: &'a [model_ingest::AgeRow],
    /// `Some` for a metadata probe.
    pub probe: Option<&'a ProbeExport>,
}

/// The name a plan node is CREATED with: its launch name, or its executable.
fn node_name(n: &PlanNode) -> &str {
    n.name.as_deref().unwrap_or(&n.exec)
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
/// `nros_board_native_run_components_named_ns` makes for the session rung.
pub(crate) fn node_namespace(n: &PlanNode) -> &str {
    match n.namespace.as_deref() {
        None | Some("") => "/",
        Some(ns) => ns,
    }
}

/// How a plan node's component is constructed — the one reading of the cmake
/// metadata's `lang` and `shape` (RFC-0043 / RFC-0044 / phase-257).
///
/// A `C` node is reached through its `NROS_C_COMPONENT` seam, a `Rust` node
/// through its install seam; anything else is a C++ class, constructed with a
/// handle when its shape is `rclcpp` and default-constructed then
/// `configure`d otherwise (`None` included — the 240.x back-compat default).
pub(crate) fn component_kind(n: &PlanNode) -> ComponentKind {
    match n.lang {
        Some(Lang::C) => ComponentKind::C,
        Some(Lang::Rust) => ComponentKind::Rust,
        Some(Lang::Cpp) | None => {
            if n.shape.as_deref() == Some("rclcpp") {
                ComponentKind::Rclcpp
            } else {
                ComponentKind::Configure
            }
        }
    }
}

/// One node, both halves: the per-node runtime bake (issue 0302) and what a
/// C-ABI pack reads of it.
///
/// Four bake features arrived over four phases — params (264 W4a), identity
/// (268 W1), remaps (305 W3 / issue 0255), QoS overrides (issue #52) — and
/// each wired the proc-macro while leaving the CLI emitter behind, so a
/// CLI-baked entry ran every node with default parameters, no remaps, its own
/// hardcoded name and no QoS overrides. From the same plan. That is the drift
/// the shared [`LoweredNode`] and the parity corpus exist to make impossible.
fn lower_node(n: &PlanNode) -> LoweredNode {
    let kind = component_kind(n);
    LoweredNode {
        pkg: n.pkg.clone(),
        params: n.params.clone(),
        remaps: n.remaps.clone(),
        // The plan carries LOWERED codes: `nros_orchestration_ir::qos_override`
        // already rejected anything unusable (issue 0303), so nothing is
        // decoded or silently dropped here.
        qos_overrides: n
            .qos_overrides
            .iter()
            .map(|o| QosOverride {
                topic: o.topic.clone(),
                role: o.role,
                policy: o.policy,
                value: o.value,
            })
            .collect(),
        // A namespace without a name is not an identity: the proc-macro keys
        // the override on the name, so `None` here means "keep the node's own".
        identity: n
            .name
            .as_ref()
            .map(|name| NodeIdentity::new(name, n.namespace.as_deref().unwrap_or(""))),
        name: node_name(n).to_string(),
        namespace: node_namespace(n).to_string(),
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
        kind: Some(kind),
        class: if kind.is_cpp_class() {
            n.class_name.clone()
        } else {
            None
        },
    }
}

/// Stage 2 for the RUST producers: the per-node bake both of them render.
///
/// The proc-macro has no `Plan` (RFC-0091 §4 — `Plan` is the CLI's own
/// projection of its input), so what is shared with it is the OUTPUT type; the
/// parity corpus proves the two converge there. The image half stays empty:
/// a Rust entry reads none of it, and a Rust-only board key has no family.
pub fn lower_entry(plan: &Plan) -> LoweredEntry {
    LoweredEntry {
        bringup: plan.bringup.clone(),
        launch: plan.launch_file.display().to_string(),
        board: plan.board.clone(),
        // include_bytes! tracking — same rebuild-correctness workaround the
        // proc-macro uses. A path that does not exist is skipped, exactly as
        // the proc-macro does: `include_bytes!` on a missing path is a hard
        // compile error, and the pkg-index walk can name a synthesised dir.
        depfiles: plan
            .depfile_paths
            .iter()
            .filter(|d| d.exists())
            .map(|d| d.display().to_string())
            .collect(),
        nodes: plan.nodes.iter().map(lower_node).collect(),
        ..Default::default()
    }
}

/// Stage 2 for the C-ABI entry packs: [`lower_entry`] plus the image.
///
/// # Errors
///
/// A message naming what the plan cannot express — the board key, the node,
/// the tier or the boot-config field — in the order a reader would fix them.
pub fn lower_image(plan: &Plan, opts: &LowerOptions<'_>) -> Result<LoweredEntry, String> {
    // Issue 1285 — an unknown board key is refused HERE, naming the known
    // ones, before anything reads the family.
    let family = nros_entry_lower::board_family(&plan.board)
        .map_err(|e| format!("typed entry emit: {e}"))?;

    for n in &plan.nodes {
        let kind = component_kind(n);
        // A C or Rust node has no C++ class, so the C++ class's two facts
        // are required of the class kinds alone (phase-432 W2.6 — requiring
        // them of a C node made cmake fill in a field nothing reads).
        if kind.is_cpp_class() && n.class_name.is_none() {
            return Err(format!(
                "typed entry emit: node pkg `{}` exec `{}` is missing class_name (cmake metadata)",
                n.pkg, n.exec
            ));
        }
        if kind.is_cpp_class() && n.class_header.is_none() {
            return Err(format!(
                "typed entry emit: C++ node pkg `{}` exec `{}` is missing class_header \
                 — the typed Entry needs the component's class header (cmake metadata)",
                n.pkg, n.exec
            ));
        }
    }

    let mut e = lower_entry(plan);
    e.family = Some(family);
    e.boot_shape = Some(family.boot_shape());

    // Forward declarations are per PACKAGE, storage is per NODE: one seam per
    // (identifier, kind), first-seen order. Keyed on the IDENTIFIER, because
    // two spellings that sanitise alike declare one symbol.
    for n in &plan.nodes {
        let kind = component_kind(n);
        let ident = super::sanitize_pkg(&n.pkg);
        if !e
            .components
            .iter()
            .any(|c| c.kind == kind && super::sanitize_pkg(&c.pkg) == ident)
        {
            e.components.push(ComponentSeam {
                pkg: n.pkg.clone(),
                kind,
            });
        }
        // One `#include` per unique C++ component header (first-seen order).
        if kind.is_cpp_class() {
            let h = n.class_header.clone().expect("validated above");
            if !e.headers.contains(&h) {
                e.headers.push(h);
            }
        }
    }

    // issue 1283 — the branch is the PLAN's (`Plan::executor_shape`): a
    // multi-tier plan takes `run_tiers` unless a node's groups span tiers or
    // the board has none, which keep the single-executor sched-context path.
    //
    // phase-308 W1 — the one refinement is the PROBE's: it always takes the
    // single-setup shape. Tiers would be worse than irrelevant —
    // `create_entity` early-returns for entities whose callback group is
    // inactive on the running tier, so a per-tier probe would UNDER-count
    // exactly what the sidecar exists to count.
    let shape = match (plan.executor_shape(), opts.probe) {
        (ExecutorShape::Tiers, Some(_)) => ExecutorShape::SchedContexts,
        (shape, _) => shape,
    };

    if shape == ExecutorShape::Tiers {
        let tiers = plan.resolved_tiers.as_ref().expect("Tiers implies a table");
        e.tiers = Some(lower_tiers(plan, tiers, opts)?);
        e.monitor_tables = e
            .tiers
            .iter()
            .flat_map(|t| t.setups.iter().filter_map(|s| s.monitors.clone()))
            .collect();
    } else {
        if shape == ExecutorShape::SchedContexts {
            let tiers = plan
                .resolved_tiers
                .as_ref()
                .expect("SchedContexts implies a table");
            e.sched = Some(lower_sched(tiers, plan));
        }
        let nodes: Vec<String> = plan.nodes.iter().map(plan_node_fqn).collect();
        e.monitors = monitor_table(None, opts, |node| nodes.iter().any(|n| n == node));
        e.monitor_tables.extend(e.monitors.iter().cloned());
    }

    e.services = LoweredServices {
        param_services: plan.param_services,
        lifecycle_code: plan.lifecycle.as_deref().map(|a| match a {
            "none" => 0u8,
            "configure" => 1,
            _ => 2,
        }),
    };

    match opts.probe {
        // A probe returns before the boot config and the board wrapper: it
        // opens a session against the recording backend, runs setup once,
        // and dumps.
        Some(p) => {
            e.probe = Some(LoweredProbe {
                package: p.package.clone(),
                component: p.component.clone(),
                executable: p.executable.clone(),
                // The one producer of the rendered spelling (phase-469).
                language: p.language.as_str().to_string(),
                out_path: p.out_path.clone(),
            });
        }
        None => {
            e.boot_config = Some(boot_config(plan)?);
            let runners: LoweredRunners =
                family.c_abi_runners().map(Into::into).unwrap_or_default();
            let raw = family.c_abi_runners();
            let tiered = e.tiers.is_some();
            e.boot = Some(LoweredBoot {
                tier_storage: tiered && raw.is_some_and(|r| r.tiers_take_static_storage()),
                component_storage: !tiered
                    && raw.is_some_and(|r| r.components_take_static_storage()),
                n_tiers: e.tiers.as_ref().map_or(0, |t| t.n),
                tiers: tiered,
                runners,
            });
        }
    }
    Ok(e)
}

/// The `run_tiers` layout: one setup per tier, and the tier table.
fn lower_tiers(
    plan: &Plan,
    tiers: &ResolvedTierTable,
    opts: &LowerOptions<'_>,
) -> Result<LoweredTiers, String> {
    // #0266 — the ThreadX round-robin time slice has a per-thread consumer only
    // on the Rust `nros::main!` ThreadX arm today. The C tier ABI
    // (`nros_native_tier_spec_t`) carries no time-slice field, so a lowered
    // row has no slot for it, and rather than silently drop a declared value
    // the lowering fails loud.
    if let Some(t) = tiers.tiers.iter().find(|t| t.time_slice_us.is_some()) {
        return Err(format!(
            "tier '{}': time_slice_us is not yet supported on the C/C++ codegen \
             path (#0266) — declare it only on a Rust `nros::main!` ThreadX entry, \
             or file the C consumer",
            t.name
        ));
    }

    // node name → tier index, for per-tier node filtering.
    let node_to_tier: HashMap<&str, usize> = tiers
        .tiers
        .iter()
        .enumerate()
        .flat_map(|(ti, tier)| {
            tier.members
                .iter()
                .map(move |(node, _)| (node.as_str(), ti))
        })
        .collect();

    let setups = tiers
        .tiers
        .iter()
        .enumerate()
        .map(|(ti, tier)| {
            // phase-462 W1 — this tier's monitor rows: those of the nodes
            // the tier NAMES, so a row is installed on exactly the executor
            // that constructs its node and never checked twice.
            let members: Vec<String> = plan
                .nodes
                .iter()
                .filter(|n| tier.members.iter().any(|(m, _)| m == node_name(n)))
                .map(plan_node_fqn)
                .collect();
            TierSetup {
                index: ti,
                name: tier.name.clone(),
                nodes: plan
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| node_to_tier.get(node_name(n)).copied() == Some(ti))
                    // issue 1272 — each tier setup builds its nodes on the
                    // tier's OWN executor, so the index restarts per tier.
                    .enumerate()
                    .map(|(k, (i, _))| SetupNode {
                        node: i,
                        on_executor: k,
                    })
                    .collect(),
                emits_services: ti == 0,
                monitors: monitor_table(Some((ti, &tier.name)), opts, |node| {
                    members.iter().any(|n| n == node)
                }),
            }
        })
        .collect();

    let groups = tier_group_keys(tiers, plan);
    let rows = tiers
        .tiers
        .iter()
        .enumerate()
        .map(|(ti, tier)| TierRow {
            index: ti,
            name: tier.name.clone(),
            groups: groups[ti].clone(),
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
        .collect();

    Ok(LoweredTiers {
        n: tiers.tiers.len(),
        setups,
        rows,
    })
}

/// The namespace of the plan node a tier member names (`"/"` if none does).
fn member_namespace(plan: &Plan, member: &str) -> String {
    plan.nodes
        .iter()
        .find(|n| node_name(n) == member)
        .map(node_namespace)
        .unwrap_or("/")
        .to_string()
}

/// The `(node name, namespace, group)` key for every group admitted on each
/// tier — ONE derivation.
///
/// issue 1172 — this used to be written twice and differently. `emit_c` deduped
/// ACROSS tiers, so a group named by two tiers left the SECOND tier's array
/// empty; an empty array is the WILDCARD (`main.h`: "NULL / 0 means wildcard"),
/// so that tier stopped filtering and ran every callback in the image at its
/// own priority. `emit_cpp` deduped WITHIN each tier and kept empty ids.
/// With the node in the key there is nothing to dedup across tiers: two nodes'
/// `ctrl` are two different keys.
///
/// The namespace MUST be the one the entry creates the node with — a filter
/// naming a namespace the node does not have matches nothing.
pub(crate) fn tier_group_keys(
    tiers: &ResolvedTierTable,
    plan: &Plan,
) -> Vec<Vec<(String, String, String)>> {
    tiers
        .tiers
        .iter()
        .map(|tier| {
            let mut keys: Vec<(String, String, String)> = tier
                .members
                .iter()
                // An empty group id names no group and never did; emitting it
                // would make the tier match an entity whose group is "".
                .filter(|(_, group)| !group.is_empty())
                .map(|(node, group)| (node.clone(), member_namespace(plan, node), group.clone()))
                .collect();
            // Only an exact repeat of the same triple is a duplicate.
            keys.sort();
            keys.dedup();
            keys
        })
        .collect()
}

/// The sched-context wiring for [`ExecutorShape::SchedContexts`] — ONE
/// derivation (issue 1283). It used to live in `emit_cpp` alone, so the C pack
/// had nothing to render and emitted nothing.
fn lower_sched(tiers: &ResolvedTierTable, plan: &Plan) -> LoweredSched {
    LoweredSched {
        n: tiers.tiers.len(),
        contexts: tiers
            .tiers
            .iter()
            .enumerate()
            .map(|(ti, tier)| SchedContext {
                index: ti,
                class: tier.class.clone(),
                period_us: tier.period_us.unwrap_or(0),
                budget_us: tier.budget_us.unwrap_or(0),
                deadline_us: tier.deadline_us.unwrap_or(0),
                deadline_policy: tier.deadline_policy.clone(),
                os_pri: tier.priority.clamp(0, 255) as u8,
            })
            .collect(),
        node_binds: plan
            .nodes
            .iter()
            .filter_map(|n| {
                n.sched_context.map(|sc| NodeBind {
                    name: node_name(n).to_string(),
                    namespace: node_namespace(n).to_string(),
                    sched_context: sc,
                })
            })
            .collect(),
        group_binds: tiers
            .tiers
            .iter()
            .enumerate()
            .flat_map(|(ti, tier)| {
                tier.members.iter().map(move |(node, group)| GroupBind {
                    namespace: member_namespace(plan, node),
                    name: node.clone(),
                    group: group.clone(),
                    tier_index: ti,
                })
            })
            .collect(),
    }
}

/// The node FQN a plan node registers under (`<namespace>/<name>`), the key a
/// contract row's `fqn` starts with. The namespace half is [`node_namespace`],
/// so a row's key and the node the entry creates cannot disagree about what
/// "no namespace" means (issue 1443).
fn plan_node_fqn(n: &PlanNode) -> String {
    let name = node_name(n);
    match node_namespace(n) {
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

/// The monitor rows whose node `keep` admits, in row order; `None` when there
/// are none. phase-462 W1 — the model's rows cover every node of the SYSTEM,
/// so a row is kept only when its node is one this executor constructs.
fn monitor_table(
    tier: Option<(usize, &str)>,
    opts: &LowerOptions<'_>,
    keep: impl Fn(&str) -> bool,
) -> Option<MonitorTable> {
    let t = MonitorTable {
        tier: tier.map(|(i, _)| i),
        tier_name: tier.map(|(_, n)| n.to_string()),
        rows: opts
            .monitors
            .iter()
            .filter(|r| keep(row_node_fqn(&r.fqn)))
            .map(|r| MonitorRow {
                topic: r.topic.clone(),
                fqn: r.fqn.clone(),
                min_rate_hz_milli: r.min_rate_hz_milli,
                max_latency_ms: r.max_latency_ms,
            })
            .collect(),
        ages: opts
            .ages
            .iter()
            .filter(|r| keep(row_node_fqn(&r.fqn)))
            .map(|r| AgeRow {
                topic: r.topic.clone(),
                fqn: r.fqn.clone(),
                max_age_ms: r.max_age_ms,
            })
            .collect(),
    };
    (!t.is_empty()).then_some(t)
}

/// The blob's five facts, refusing a value the C field cannot hold.
///
/// Phase 266 (W5b/W6) / issue 0794. The facts split into two kinds, and the
/// kind decides what a multi-node image gets:
///
/// * **Identity** — `node_name`, `namespace`. Per NODE, so a multi-node plan
///   (or a single node with no resolvable name) leaves both `None`: the flags
///   come out clear, `nros_boot_config_node_name` returns NULL, and the runner
///   falls back to the unified `"node"` default.
/// * **Session** — `domain`, `locator`, `rmw`. Per IMAGE, so they survive a
///   multi-node plan whenever every deployed node agrees on them
///   ([`super::BakedSession`]).
///
/// # Errors
///
/// When a resolved string exceeds its fixed C buffer minus the NUL:
/// `node_name` and `namespace_` are `char [64]` (63 bytes), `locator` is
/// `char [96]` (95) and `rmw` is `char [32]` (31). The caller gets a clear
/// diagnostic instead of a confusing C-compiler array-initialiser error. This
/// is a correctness check, so it stays in compiled Rust rather than moving into
/// the template with the layout.
///
/// The domain is NOT range-checked here. `DOMAIN_ID_MAX` lives in `nros-node`,
/// which this crate does not depend on, and mirroring the constant would be a
/// second authored copy of a cap the resolver already enforces at boot
/// (`BootConfigError::DomainIdRange`).
pub(crate) fn boot_config(plan: &Plan) -> Result<LoweredBootConfig, String> {
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

    let s = &plan.session;
    if let Some(loc) = s.locator.as_deref() {
        fits("locator", loc, "locator", 95)?;
    }
    if let Some(rmw) = s.rmw.as_deref() {
        fits("rmw", rmw, "rmw", 31)?;
    }
    let mut view = LoweredBootConfig {
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
    let raw = node_name(n);
    fits("name", raw, "node_name", 63)?;
    if let Some(ns) = n.namespace.as_deref() {
        fits("namespace", ns, "namespace_", 63)?;
        view.namespace = Some(ns.to_string());
    }
    view.node_name = Some(raw.to_string());
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kind's spelling IS its serde repr — what a manifest's
    /// `components` list names and what a template compares against.
    #[test]
    fn a_component_kind_is_spelled_as_it_serialises() {
        for k in ComponentKind::ALL {
            assert_eq!(
                serde_json::to_value(k).unwrap(),
                serde_json::Value::String(k.as_str().into())
            );
        }
    }
}
