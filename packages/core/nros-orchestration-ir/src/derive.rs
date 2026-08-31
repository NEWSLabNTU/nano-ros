//! phase-296 W5.13 follow-up — the DERIVED-schedule core (RFC-0052).
//!
//! When a resolved `SystemModel` declares NO `execution.tiers`, the schedule is
//! DERIVED from the contract layer: `mapper_input_from_model` → `chain_aware_rank`
//! → `realize_rtos` produce a per-node RTOS realization, which this module
//! synthesizes into ordinary `[tiers.*]` (`derived-<node>`) + `[[node_overrides]]`
//! rows. Both consumers call this ONE core so the derivation never drifts:
//! - the CLI's `codegen-system` (wraps it with `SystemToml` mutation), and
//! - the `nros::main!` proc-macro (feeds the tiers straight into `resolve_tiers`).
//!
//! The core is side-effect free: it RETURNS the degradations + groupless notes
//! rather than printing them, so each caller surfaces them in its own channel
//! (the CLI to stderr, the macro as compile diagnostics).

use std::collections::BTreeMap;

use ros_launch_manifest_model::SystemModel;
use ros_launch_manifest_sched::chain_aware_rank;

use crate::{
    CallbackGroupDecl, CallbackGroupOverride, NodeOverride, TierDef, TierRtosSpec,
    mapper_input::mapper_input_from_model,
    rtos_realizer::{Degradation, realize_rtos, sched_caps_for},
};

/// The outcome of deriving a schedule from the contract layer.
#[derive(Debug, Default)]
pub struct DerivedSchedule {
    /// `derived-<node>` tier definitions, keyed by tier name.
    pub tiers: BTreeMap<String, TierDef>,
    /// One override per derived node, binding its callback groups to its tier.
    pub overrides: Vec<NodeOverride>,
    /// Every guarantee weakening the realizer recorded (fail-loud — the caller
    /// MUST surface these).
    pub degradations: Vec<Degradation>,
    /// Nodes that ranked but declare no callback groups — they stay on the
    /// default tier (advisory-derived, not authored). One human-readable note
    /// per node; the caller surfaces them.
    pub groupless_notes: Vec<String>,
}

/// Bare node name from a model FQN (`/ns/node` → `node`).
fn bare(fqn: &str) -> &str {
    fqn.rsplit('/').next().unwrap_or(fqn)
}

/// Derive a per-node RTOS schedule from the model's contract layer. Returns an
/// empty [`DerivedSchedule`] (no tiers) when nothing is schedulable — the caller
/// then bakes tier-less exactly as before. `callback_groups` maps a bare node
/// name to its declared groups (from cargo/cmake metadata); a node with none
/// stays on the default tier.
pub fn derive_tiers_from_contracts(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
) -> DerivedSchedule {
    let input = mapper_input_from_model(model);
    let ranked = chain_aware_rank(&input);
    if ranked.items.is_empty() {
        return DerivedSchedule::default();
    }

    // Issue 0951 — the per-deploy `edf` / `cores` knobs are GONE, because they
    // were never reachable.
    //
    // They read `Deploy.extra`, and no AUTHORING path writes those keys. The
    // resolver's placement branches write `host_name` on the host path and
    // kind/target/framework/profile/optimize/features/deploy_name on the
    // deprecated deploy one — never `edf` or `cores` — and upstream's
    // `DeployBlock` is `deny_unknown_fields`, so a `system.toml` cannot carry
    // them either.
    //
    // Precisely: the only way to reach the knob was to hand-edit a resolved
    // model, and a model is a BUILD ARTIFACT that `check-no-tracked-models`
    // forbids committing (issue 0380 was exactly four such hand-edits). So the
    // knob was reachable, but only from a file nobody is allowed to keep — and
    // the "unanimous-or-error" check guarding it therefore guarded an override
    // that no supported input could request.
    //
    // Issue 0259 wants exactly this override to exist. Whoever builds it needs
    // an AUTHORING path first (a real table with a real reader), not this
    // plumbing — which is why deleting it is the honest move: keeping it made
    // a knob that does nothing look supported.
    let caps = sched_caps_for(target_rtos);

    let plan = realize_rtos(&ranked, &input, &caps);
    let mut out = DerivedSchedule {
        degradations: plan.degradations.clone(),
        ..Default::default()
    };

    for n in &plan.nodes {
        let node = bare(&n.name).to_string();
        // A ranked node with no declared callback groups has nothing for the
        // gating executor to bind — it stays on the default tier (loud note).
        let Some(groups) = callback_groups.get(&node).filter(|g| !g.is_empty()) else {
            out.groupless_notes.push(n.name.clone());
            continue;
        };
        let tier_name = format!("derived{}", n.name.replace('/', "-"));
        let spec = TierRtosSpec {
            priority: n.priority,
            stack_bytes: None,
            preempt_threshold: n.preempt_threshold,
            // Time-slicing is not a derived dim (no contract fact implies
            // round-robin); an authored `time_slice_us` is the only source.
            time_slice_us: None,
            // phase-330 W1.a — placement/policy dims are AUTHORED, never
            // derived: nothing in a contract fact implies a core pin or a
            // sporadic budget. Left unset so a derived tier stays a plain
            // priority tier.
            core: None,
            deadline_us: None,
            budget_us: None,
            period_us: None,
            // sched_class deliberately unset: the runtime consumes the GENERIC
            // policy (class/period/budget/deadline → SchedContext; on Zephyr
            // class+deadline → kernel EDF). The realizer's internal vocab
            // ("edf"/"sporadic") is not the sub-table's POSIX-class vocab.
            sched_class: None,
        };
        let mut def = TierDef {
            class: Some(
                if n.deadline_us.is_some() || n.budget_us.is_some() {
                    "real_time"
                } else {
                    "best_effort"
                }
                .to_string(),
            ),
            period_us: n.period_us,
            budget_us: n.budget_us,
            deadline_us: n.deadline_us,
            core: n.core,
            ..Default::default()
        };
        match target_rtos {
            t if t.contains("zephyr") => def.zephyr = Some(spec),
            t if t.contains("freertos") => def.freertos = Some(spec),
            t if t.contains("threadx") => def.threadx = Some(spec),
            t if t.contains("nuttx") => def.nuttx = Some(spec),
            _ => def.posix = Some(spec),
        }
        out.tiers.insert(tier_name.clone(), def);
        out.overrides.push(NodeOverride {
            name: node,
            callback_groups: groups
                .iter()
                .map(|g| CallbackGroupOverride {
                    id: g.id.clone(),
                    tier: tier_name.clone(),
                })
                .collect(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ros_launch_manifest_model::{
        Contracts, NodeInstance, PathContract, PubContract, Structure, SystemModel, TopicWiring,
    };

    use super::*;
    use crate::{CallbackGroupDecl, resolve_tiers};

    /// A contract-only model: control is a tight 5 ms / 100 Hz loop, telem a
    /// slack 100 ms / 10 Hz loop, NO `execution.tiers`.
    fn contract_model() -> SystemModel {
        let node = |scope: &str| NodeInstance {
            scope: scope.into(),
            ..Default::default()
        };
        let mut nodes = indexmap::IndexMap::new();
        nodes.insert("/control_node".to_string(), node("/"));
        nodes.insert("/telem_node".to_string(), node("/"));
        let mut topics = std::collections::BTreeMap::new();
        topics.insert(
            "/cmd".to_string(),
            TopicWiring {
                msg_type: "std_msgs/Int32".into(),
                publishers: vec!["/control_node/cmd".into()],
                subscribers: vec![],
            },
        );
        topics.insert(
            "/status".to_string(),
            TopicWiring {
                msg_type: "std_msgs/Int32".into(),
                publishers: vec!["/telem_node/status".into()],
                subscribers: vec![],
            },
        );
        let mut node_paths = std::collections::BTreeMap::new();
        node_paths.insert(
            "/control_node/loop".to_string(),
            PathContract {
                input: vec![],
                output: vec!["/control_node/cmd".into()],
                max_latency_ms: Some(5.0),
                ..Default::default()
            },
        );
        node_paths.insert(
            "/telem_node/loop".to_string(),
            PathContract {
                input: vec![],
                output: vec!["/telem_node/status".into()],
                max_latency_ms: Some(100.0),
                ..Default::default()
            },
        );
        let mut pub_endpoints = std::collections::BTreeMap::new();
        pub_endpoints.insert(
            "/control_node/cmd".to_string(),
            PubContract {
                min_rate_hz: Some(100.0),
                ..Default::default()
            },
        );
        pub_endpoints.insert(
            "/telem_node/status".to_string(),
            PubContract {
                min_rate_hz: Some(10.0),
                ..Default::default()
            },
        );
        SystemModel {
            structure: Structure {
                nodes,
                topics,
                ..Default::default()
            },
            contracts: Contracts {
                node_paths,
                pub_endpoints,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn cbg(id: &str, tier: &str) -> CallbackGroupDecl {
        CallbackGroupDecl {
            id: id.to_string(),
            r#type: "MutuallyExclusive".to_string(),
            tier: tier.to_string(),
        }
    }

    /// The EXACT sequence `nros::main!` runs for a tier-less model: derive →
    /// resolve_tiers(derived.tiers, derived.overrides, …). The authored group
    /// tiers ("high"/"low") do NOT exist in the derived table, so this also
    /// proves the overrides fully REBIND groups onto the `derived-<node>` tiers.
    #[test]
    fn derive_then_resolve_matches_the_macro_path() {
        let model = contract_model();
        let mut groups: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::new();
        groups.insert("control_node".into(), vec![cbg("ctrl", "high")]);
        groups.insert("telem_node".into(), vec![cbg("telem", "low")]);

        let derived = derive_tiers_from_contracts(&model, "posix", &groups);
        assert!(derived.tiers.contains_key("derived-control_node"));
        assert!(derived.tiers.contains_key("derived-telem_node"));

        let component_names: BTreeSet<&str> = ["control_node", "telem_node"].into_iter().collect();
        let table = resolve_tiers(
            &derived.tiers,
            &derived.overrides,
            &component_names,
            &groups,
            "posix",
        )
        .expect("derived table resolves (overrides rebind the authored group tiers)");
        // Highest-priority-first: control's 5 ms deadline outranks telem's 100 ms.
        let ctrl = table
            .tiers
            .iter()
            .position(|t| t.name == "derived-control_node")
            .expect("control tier present");
        let telem = table
            .tiers
            .iter()
            .position(|t| t.name == "derived-telem_node")
            .expect("telem tier present");
        assert!(
            ctrl < telem,
            "control tier must precede telem: {:?}",
            table.tiers
        );
    }

    /// A tier-less, contract-less model derives nothing (byte-identical to the
    /// pre-derive tier-less bake).
    #[test]
    fn no_contracts_derives_nothing() {
        let derived =
            derive_tiers_from_contracts(&SystemModel::default(), "posix", &BTreeMap::new());
        assert!(derived.tiers.is_empty());
        assert!(derived.overrides.is_empty());
    }
}
