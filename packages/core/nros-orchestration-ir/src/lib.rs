//! Shared per-tier orchestration IR — schema + resolver (RFC-0015).
//!
//! This leaf crate holds the small set of `system.toml` types that
//! describe scheduling tiers + the [`resolve_tiers`] algorithm that
//! lowers them (with `[[node_overrides]]` applied) into an ordered,
//! per-RTOS **resolved tier table**. It is depended on by BOTH:
//!
//! - the `nros` CLI (`nros-cli-core`), whose `codegen-system` bakes the
//!   resolved table into `nros-plan.json`, and
//! - the `nros::main!()` proc-macro (`nros-macros`), which resolves the
//!   same table at compile time to emit one task/`Executor` per tier.
//!
//! Keeping the schema + resolver here is the single source of truth so
//! the build-time and codegen paths can never drift. The crate is pure
//! host code (serde + thiserror); it carries no runtime/`no_std`
//! dependencies.
//!
//! The all-default-tier degenerate case (no `[tiers.*]`, no callback
//! groups) resolves to a single synthesized `"default"` tier — the
//! single-task shape that ships today.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// =============================================================================
// system.toml schema (tier subset)
// =============================================================================

/// `[tiers.<name>]` — a symbolic priority tier (RFC-0015 §4.2). Carries the
/// RTOS-agnostic `spin_period_us` plus a per-RTOS sub-table
/// (`[tiers.<name>.<rtos>]`) giving the concrete priority/stack for each target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spin_period_us: Option<u64>,
    // Phase 256 W4 (decision A) — the RTOS-AGNOSTIC real-time policy a callback
    // group runs under (absorbed from the retired `[[scheduling.contexts]]`
    // overlay). Per-RTOS placement (priority/stack) stays in the `<rtos>`
    // sub-tables; these describe HOW it is scheduled, identically on every RTOS.
    // All optional → a plain priority tier (today's shape) is byte-identical.
    /// Scheduling class — the plan's `SchedClass` (snake_case): `"best_effort"` |
    /// `"real_time"` (default for a priority tier) | `"time_triggered"` |
    /// `"interrupt"`. The W4.2 codegen lowering validates + maps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Callback period (µs) for `periodic` / `time_triggered`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_us: Option<u64>,
    /// Execution-time budget (µs) — EDF/sporadic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_us: Option<u64>,
    /// Relative deadline (µs) — EDF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_us: Option<u64>,
    /// On deadline miss — the plan's `DeadlinePolicy` (snake_case): `"ignore"`
    /// (default) | `"warn"` | `"skip"` | `"fault"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_policy: Option<String>,
    /// CPU core to pin the tier task to (SMP); `None` ⇒ unpinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freertos: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zephyr: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threadx: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nuttx: Option<TierRtosSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posix: Option<TierRtosSpec>,
}

/// `[tiers.<name>.<rtos>]` — concrete per-RTOS task knobs. One shape for all
/// RTOSes; `priority` is `i64` to admit Zephyr's negative coop priorities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierRtosSpec {
    pub priority: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_bytes: Option<u32>,
    /// ThreadX preemption threshold (ignored on other RTOSes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preempt_threshold: Option<i64>,
    /// POSIX scheduler class (e.g. `"SCHED_FIFO"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sched_class: Option<String>,
}

/// `[[node.callback_groups]]` row (Phase 228.A, RFC-0015 §4.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackGroupDecl {
    /// Logical id within the node (e.g. `"ctrl_loop"`, `"telemetry"`).
    pub id: String,
    /// `"MutuallyExclusive"` (default) or `"Reentrant"`. v1 treats every group
    /// as mutually-exclusive within its tier task; the field is recorded for
    /// the future multi-worker executor.
    #[serde(default = "default_cbg_type")]
    pub r#type: String,
    /// Symbolic tier name resolved against the system's `[tiers.*]`.
    #[serde(default = "default_tier_name")]
    pub tier: String,
}

fn default_cbg_type() -> String {
    "MutuallyExclusive".to_string()
}

fn default_tier_name() -> String {
    DEFAULT_TIER.to_string()
}

/// `[[node_overrides]]` row — reassigns a node's callback groups to tiers
/// at deploy time without touching the node package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeOverride {
    /// Node instance name (matches a `[[component]].name`).
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callback_groups: Vec<CallbackGroupOverride>,
}

/// A single `id → tier` reassignment inside a `[[node_overrides]]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackGroupOverride {
    pub id: String,
    pub tier: String,
}

// =============================================================================
// resolved tier table
// =============================================================================

/// The synthesized tier used when a callback group names no tier (or none are
/// declared at all). It needs no `[tiers.default]` table.
pub const DEFAULT_TIER: &str = "default";

/// One resolved tier: a concrete RTOS task to emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTier {
    pub name: String,
    /// RTOS-specific numeric priority (the spawn-call value).
    pub priority: i64,
    pub stack_bytes: Option<u32>,
    pub spin_period_us: Option<u64>,
    pub preempt_threshold: Option<i64>,
    pub sched_class: Option<String>,
    // Phase 256 W4 — the RTOS-agnostic real-time policy (from `TierDef`), carried
    // through so the planner can lower a tier to a `PlanSchedContext` (the home the
    // retired `[[scheduling.contexts]]` overlay used to fill).
    pub class: Option<String>,
    pub period_us: Option<u64>,
    pub budget_us: Option<u64>,
    pub deadline_us: Option<u64>,
    pub deadline_policy: Option<String>,
    pub core: Option<u32>,
    /// `(node_name, callback_group_id)` pairs assigned to this tier, sorted.
    pub members: Vec<(String, String)>,
}

/// The ordered tier table for one deploy target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTierTable {
    /// Tiers ordered highest-priority first (by the RTOS numeric `priority`).
    pub tiers: Vec<ResolvedTier>,
}

impl ResolvedTierTable {
    /// True when this is the single-task degenerate case (one tier, the
    /// synthesized `default`). Codegen uses this to skip multi-task scaffolding.
    pub fn is_single_tier(&self) -> bool {
        self.tiers.len() == 1 && self.tiers[0].name == DEFAULT_TIER
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TierResolveError {
    #[error(
        "callback group `{node}/{group}` names tier `{tier}`, which has no `[tiers.{tier}]` definition"
    )]
    UnknownTier {
        node: String,
        group: String,
        tier: String,
    },
    #[error("tier `{tier}` has no `[tiers.{tier}.{rtos}]` sub-table for the target RTOS")]
    MissingRtosSpec { tier: String, rtos: String },
    #[error("`[[node_overrides]]` targets node `{node}` which is not a component in the system")]
    UnknownOverrideNode { node: String },
    #[error(
        "node `{node}` has callback groups in different tiers (`{tier_a}` and `{tier_b}`); v1 \
         pins a whole node to one tier — put its groups in the same tier"
    )]
    NodeSpansTiers {
        node: String,
        tier_a: String,
        tier_b: String,
    },
}

/// Pick a tier's per-RTOS spec by target name.
fn rtos_spec<'a>(def: &'a TierDef, rtos: &str) -> Option<&'a TierRtosSpec> {
    match rtos {
        "freertos" => def.freertos.as_ref(),
        "zephyr" => def.zephyr.as_ref(),
        "threadx" => def.threadx.as_ref(),
        "nuttx" => def.nuttx.as_ref(),
        "posix" | "native" => def.posix.as_ref(),
        _ => None,
    }
}

/// Resolve a system's tiers against the per-node callback groups for one
/// target RTOS.
///
/// Inputs are the decomposed `system.toml` pieces so both the CLI (which
/// holds a full `SystemToml`) and the proc-macro (which parses a leaner
/// view) can call this without sharing the whole config type:
/// - `tiers` — the `[tiers.*]` table.
/// - `node_overrides` — the `[[node_overrides]]` rows.
/// - `component_names` — the system's component *instance* names, used to
///   validate override targets.
/// - `callback_groups` — component-instance-name → its declared groups
///   (`[package.metadata.nros.node].callback_groups`).
pub fn resolve_tiers(
    tiers: &BTreeMap<String, TierDef>,
    node_overrides: &[NodeOverride],
    component_names: &BTreeSet<&str>,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    target_rtos: &str,
) -> Result<ResolvedTierTable, TierResolveError> {
    // Per-node group→tier overrides from the system.
    let mut overrides: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for ov in node_overrides {
        if !component_names.contains(ov.name.as_str()) {
            return Err(TierResolveError::UnknownOverrideNode {
                node: ov.name.clone(),
            });
        }
        for cg in &ov.callback_groups {
            overrides.insert((ov.name.as_str(), cg.id.as_str()), cg.tier.as_str());
        }
    }

    // (node, group) → effective tier (override wins over the node's declaration).
    let mut members_by_tier: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (node, groups) in callback_groups {
        for g in groups {
            let tier = overrides
                .get(&(node.as_str(), g.id.as_str()))
                .copied()
                .unwrap_or(g.tier.as_str());
            members_by_tier
                .entry(tier.to_string())
                .or_default()
                .push((node.clone(), g.id.clone()));
        }
    }

    // v1 node-pinned-to-tier rule (RFC-0015): every callback group of a node
    // must resolve to the SAME tier, so one node = one task = one (unlocked)
    // State. (v2 with the multi-task state-sync machinery relaxes this.)
    let mut node_tier: BTreeMap<&str, &str> = BTreeMap::new();
    for (tier, members) in &members_by_tier {
        for (node, _group) in members {
            if let Some(prev) = node_tier.insert(node.as_str(), tier.as_str())
                && prev != tier.as_str()
            {
                return Err(TierResolveError::NodeSpansTiers {
                    node: node.clone(),
                    tier_a: prev.to_string(),
                    tier_b: tier.clone(),
                });
            }
        }
    }

    // Degenerate: nothing declared → a single synthesized default tier.
    if members_by_tier.is_empty() {
        return Ok(ResolvedTierTable {
            tiers: vec![default_tier(Vec::new())],
        });
    }

    let mut out = Vec::with_capacity(members_by_tier.len());
    for (name, mut members) in members_by_tier {
        members.sort();
        if name == DEFAULT_TIER && !tiers.contains_key(DEFAULT_TIER) {
            // The default tier needs no `[tiers.default]` table.
            out.push(default_tier(members));
            continue;
        }
        let def = tiers.get(&name).ok_or_else(|| {
            let (node, group) = members.first().cloned().unwrap_or_default();
            TierResolveError::UnknownTier {
                node,
                group,
                tier: name.clone(),
            }
        })?;
        let spec =
            rtos_spec(def, target_rtos).ok_or_else(|| TierResolveError::MissingRtosSpec {
                tier: name.clone(),
                rtos: target_rtos.to_string(),
            })?;
        out.push(ResolvedTier {
            name,
            priority: spec.priority,
            stack_bytes: spec.stack_bytes,
            spin_period_us: def.spin_period_us,
            preempt_threshold: spec.preempt_threshold,
            sched_class: spec.sched_class.clone(),
            class: def.class.clone(),
            period_us: def.period_us,
            budget_us: def.budget_us,
            deadline_us: def.deadline_us,
            deadline_policy: def.deadline_policy.clone(),
            core: def.core,
            members,
        });
    }

    // Highest RTOS priority first. (The system owner authors numbers correct for
    // the target RTOS's direction; v1 does not invert.)
    out.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
    Ok(ResolvedTierTable { tiers: out })
}

fn default_tier(members: Vec<(String, String)>) -> ResolvedTier {
    ResolvedTier {
        name: DEFAULT_TIER.to_string(),
        priority: 0,
        stack_bytes: None,
        spin_period_us: None,
        preempt_threshold: None,
        sched_class: None,
        class: None,
        period_us: None,
        budget_us: None,
        deadline_us: None,
        deadline_policy: None,
        core: None,
        members,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names<'a>(items: &'a [&'a str]) -> BTreeSet<&'a str> {
        items.iter().copied().collect()
    }

    fn cbg(id: &str, tier: &str) -> CallbackGroupDecl {
        CallbackGroupDecl {
            id: id.to_string(),
            r#type: "MutuallyExclusive".to_string(),
            tier: tier.to_string(),
        }
    }

    fn posix_tier(priority: i64, spin: Option<u64>, stack: Option<u32>) -> TierDef {
        TierDef {
            spin_period_us: spin,
            posix: Some(TierRtosSpec {
                priority,
                stack_bytes: stack,
                preempt_threshold: None,
                sched_class: None,
            }),
            ..Default::default()
        }
    }

    /// Phase 256 W4 (decision A) — a `[tiers.<name>]` carrying the RTOS-agnostic
    /// real-time policy parses, and `resolve_tiers` carries those fields onto the
    /// `ResolvedTier` (the data the planner lowers to a `PlanSchedContext`).
    #[test]
    fn tier_carries_rt_policy_fields() {
        let def = TierDef {
            spin_period_us: Some(1000),
            class: Some("time_triggered".to_string()),
            period_us: Some(20000),
            budget_us: Some(5000),
            deadline_us: Some(18000),
            deadline_policy: Some("fault".to_string()),
            core: Some(1),
            posix: Some(TierRtosSpec {
                priority: 80,
                stack_bytes: Some(8192),
                preempt_threshold: None,
                sched_class: None,
            }),
            ..Default::default()
        };

        let mut tiers = BTreeMap::new();
        tiers.insert("control".to_string(), def);
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("loop", "control")]);
        let table = resolve_tiers(&tiers, &[], &names(&["control_node"]), &cbgs, "posix").unwrap();

        let t = &table.tiers[0];
        assert_eq!(t.name, "control");
        assert_eq!(t.priority, 80); // per-RTOS placement
        assert_eq!(t.class.as_deref(), Some("time_triggered")); // agnostic policy
        assert_eq!(t.period_us, Some(20000));
        assert_eq!(t.budget_us, Some(5000));
        assert_eq!(t.deadline_us, Some(18000));
        assert_eq!(t.deadline_policy.as_deref(), Some("fault"));
        assert_eq!(t.core, Some(1));
    }

    #[test]
    fn no_groups_degenerates_to_single_default_tier() {
        let table = resolve_tiers(
            &BTreeMap::new(),
            &[],
            &names(&["control_node", "telem_node"]),
            &BTreeMap::new(),
            "posix",
        )
        .unwrap();
        assert!(table.is_single_tier());
        assert_eq!(table.tiers[0].name, DEFAULT_TIER);
    }

    #[test]
    fn groups_with_no_tier_table_use_default() {
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("loop", "default")]);
        let table = resolve_tiers(
            &BTreeMap::new(),
            &[],
            &names(&["control_node"]),
            &cbgs,
            "posix",
        )
        .unwrap();
        assert!(table.is_single_tier());
        assert_eq!(
            table.tiers[0].members,
            vec![("control_node".to_string(), "loop".to_string())]
        );
    }

    #[test]
    fn resolves_two_tiers_ordered_by_priority() {
        let mut tiers = BTreeMap::new();
        tiers.insert("high".to_string(), posix_tier(80, Some(1000), Some(8192)));
        tiers.insert("low".to_string(), posix_tier(10, None, None));
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        cbgs.insert("telem_node".to_string(), vec![cbg("telem", "low")]);
        let table = resolve_tiers(
            &tiers,
            &[],
            &names(&["control_node", "telem_node"]),
            &cbgs,
            "posix",
        )
        .unwrap();
        assert_eq!(table.tiers.len(), 2);
        assert_eq!(table.tiers[0].name, "high"); // highest priority first
        assert_eq!(table.tiers[0].priority, 80);
        assert_eq!(table.tiers[0].spin_period_us, Some(1000));
        assert_eq!(table.tiers[1].name, "low");
    }

    #[test]
    fn node_spanning_tiers_errors() {
        let mut tiers = BTreeMap::new();
        tiers.insert("high".to_string(), posix_tier(80, None, None));
        tiers.insert("low".to_string(), posix_tier(10, None, None));
        let mut cbgs = BTreeMap::new();
        cbgs.insert(
            "control_node".to_string(),
            vec![cbg("ctrl", "high"), cbg("telem", "low")],
        );
        let err =
            resolve_tiers(&tiers, &[], &names(&["control_node"]), &cbgs, "posix").unwrap_err();
        assert!(matches!(err, TierResolveError::NodeSpansTiers { .. }));
    }

    #[test]
    fn node_override_moves_a_group_to_another_tier() {
        let mut tiers = BTreeMap::new();
        tiers.insert("high".to_string(), posix_tier(80, None, None));
        tiers.insert("low".to_string(), posix_tier(10, None, None));
        let overrides = vec![NodeOverride {
            name: "telem_node".to_string(),
            callback_groups: vec![CallbackGroupOverride {
                id: "telem".to_string(),
                tier: "low".to_string(),
            }],
        }];
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        cbgs.insert("telem_node".to_string(), vec![cbg("telem", "high")]);
        let table = resolve_tiers(
            &tiers,
            &overrides,
            &names(&["control_node", "telem_node"]),
            &cbgs,
            "posix",
        )
        .unwrap();
        let low = table.tiers.iter().find(|t| t.name == "low").unwrap();
        assert_eq!(
            low.members,
            vec![("telem_node".to_string(), "telem".to_string())]
        );
    }

    #[test]
    fn unknown_tier_errors() {
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "ludicrous")]);
        let err = resolve_tiers(
            &BTreeMap::new(),
            &[],
            &names(&["control_node"]),
            &cbgs,
            "posix",
        )
        .unwrap_err();
        assert!(matches!(err, TierResolveError::UnknownTier { .. }));
    }

    #[test]
    fn missing_rtos_spec_errors() {
        let mut tiers = BTreeMap::new();
        tiers.insert(
            "high".to_string(),
            TierDef {
                // only freertos declared; resolving for posix must fail.
                freertos: Some(TierRtosSpec {
                    priority: 5,
                    stack_bytes: None,
                    preempt_threshold: None,
                    sched_class: None,
                }),
                ..Default::default()
            },
        );
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        let err =
            resolve_tiers(&tiers, &[], &names(&["control_node"]), &cbgs, "posix").unwrap_err();
        assert!(matches!(err, TierResolveError::MissingRtosSpec { .. }));
    }

    #[test]
    fn override_on_unknown_node_errors() {
        let overrides = vec![NodeOverride {
            name: "ghost".to_string(),
            callback_groups: vec![],
        }];
        let err = resolve_tiers(
            &BTreeMap::new(),
            &overrides,
            &names(&["control_node"]),
            &BTreeMap::new(),
            "posix",
        )
        .unwrap_err();
        assert!(matches!(err, TierResolveError::UnknownOverrideNode { .. }));
    }
}
