//! Tier resolver (Phase 228.A, RFC-0015 §3/§5).
//!
//! Resolves the symbolic priority tiers a system declares (`[tiers.*]`) +
//! each node's callback-group → tier assignments (with `[[node_overrides]]`
//! applied) into an ordered, per-RTOS **resolved tier table** that the Wave-2
//! codegen consumes to emit one task/`Executor` per tier.
//!
//! The all-default-tier degenerate case (no `[tiers.*]`, no callback groups)
//! resolves to a single synthesized `"default"` tier — the single-task shape
//! that ships today.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use super::cargo_metadata_schema::{CallbackGroupDecl, SystemToml, TierDef, TierRtosSpec};

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
         pins a whole node to one tier — put its groups in the same tier or move shared data to \
         `[[shared_state]]`"
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

/// Resolve the system's tiers against the per-node callback groups for one
/// target RTOS.
///
/// `callback_groups` maps a component *instance* name to its declared groups
/// (from `[package.metadata.nros.node].callback_groups`).
pub fn resolve_tiers(
    system: &SystemToml,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    target_rtos: &str,
) -> Result<ResolvedTierTable, TierResolveError> {
    // Component instance names, to validate override targets.
    let component_names: BTreeSet<&str> =
        system.components.iter().map(|c| c.name.as_str()).collect();

    // Per-node group→tier overrides from the system.
    let mut overrides: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for ov in &system.node_overrides {
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
    // State. Cross-tier sharing is the `[[shared_state]]` mechanism, not a
    // node's own state. (v2 with the multi-task state-sync machinery relaxes.)
    let mut node_tier: BTreeMap<&str, &str> = BTreeMap::new();
    for (tier, members) in &members_by_tier {
        for (node, _group) in members {
            if let Some(prev) = node_tier.insert(node.as_str(), tier.as_str()) {
                if prev != tier.as_str() {
                    return Err(TierResolveError::NodeSpansTiers {
                        node: node.clone(),
                        tier_a: prev.to_string(),
                        tier_b: tier.clone(),
                    });
                }
            }
        }
    }

    // Degenerate: nothing declared → a single synthesized default tier.
    if members_by_tier.is_empty() {
        return Ok(ResolvedTierTable {
            tiers: vec![default_tier(Vec::new())],
        });
    }

    let mut tiers = Vec::with_capacity(members_by_tier.len());
    for (name, mut members) in members_by_tier {
        members.sort();
        if name == DEFAULT_TIER && !system.tiers.contains_key(DEFAULT_TIER) {
            // The default tier needs no `[tiers.default]` table.
            tiers.push(default_tier(members));
            continue;
        }
        let def = system.tiers.get(&name).ok_or_else(|| {
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
        tiers.push(ResolvedTier {
            name,
            priority: spec.priority,
            stack_bytes: spec.stack_bytes,
            spin_period_us: def.spin_period_us,
            preempt_threshold: spec.preempt_threshold,
            sched_class: spec.sched_class.clone(),
            members,
        });
    }

    // Highest RTOS priority first. (The system owner authors numbers correct for
    // the target RTOS's direction; v1 does not invert.)
    tiers.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
    Ok(ResolvedTierTable { tiers })
}

fn default_tier(members: Vec<(String, String)>) -> ResolvedTier {
    ResolvedTier {
        name: DEFAULT_TIER.to_string(),
        priority: 0,
        stack_bytes: None,
        spin_period_us: None,
        preempt_threshold: None,
        sched_class: None,
        members,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::cargo_metadata_schema::{SystemComponentEntry, SystemHeader};

    fn sys(name: &str) -> SystemToml {
        SystemToml {
            system: SystemHeader {
                name: name.to_string(),
                rmw: "zenoh".to_string(),
                domain_id: 0,
                locator: None,
                default_launch: None,
                default_target: None,
            },
            components: vec![
                SystemComponentEntry {
                    pkg: "ctrl_pkg".to_string(),
                    class: "ctrl_pkg::Control".to_string(),
                    name: "control_node".to_string(),
                },
                SystemComponentEntry {
                    pkg: "telem_pkg".to_string(),
                    class: "telem_pkg::Telem".to_string(),
                    name: "telem_node".to_string(),
                },
            ],
            deploy: BTreeMap::new(),
            domains: Vec::new(),
            bridges: Vec::new(),
            tiers: BTreeMap::new(),
            shared_state: Vec::new(),
            node_overrides: Vec::new(),
        }
    }

    fn cbg(id: &str, tier: &str) -> CallbackGroupDecl {
        CallbackGroupDecl {
            id: id.to_string(),
            r#type: "MutuallyExclusive".to_string(),
            tier: tier.to_string(),
        }
    }

    #[test]
    fn no_groups_degenerates_to_single_default_tier() {
        let table = resolve_tiers(&sys("d"), &BTreeMap::new(), "posix").unwrap();
        assert!(table.is_single_tier());
        assert_eq!(table.tiers[0].name, DEFAULT_TIER);
    }

    #[test]
    fn groups_with_no_tier_table_use_default() {
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("loop", "default")]);
        let table = resolve_tiers(&sys("d"), &cbgs, "posix").unwrap();
        assert!(table.is_single_tier());
        assert_eq!(
            table.tiers[0].members,
            vec![("control_node".to_string(), "loop".to_string())]
        );
    }

    #[test]
    fn resolves_two_tiers_ordered_by_priority() {
        let mut s = sys("d");
        s.tiers.insert(
            "high".to_string(),
            TierDef {
                spin_period_us: Some(1000),
                posix: Some(TierRtosSpec {
                    priority: 80,
                    stack_bytes: Some(8192),
                    preempt_threshold: None,
                    sched_class: Some("SCHED_FIFO".to_string()),
                }),
                ..Default::default()
            },
        );
        s.tiers.insert(
            "low".to_string(),
            TierDef {
                posix: Some(TierRtosSpec {
                    priority: 10,
                    stack_bytes: None,
                    preempt_threshold: None,
                    sched_class: None,
                }),
                ..Default::default()
            },
        );
        // Node-pinned: control_node → high, telem_node → low (each one tier).
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        cbgs.insert("telem_node".to_string(), vec![cbg("telem", "low")]);
        let table = resolve_tiers(&s, &cbgs, "posix").unwrap();
        assert_eq!(table.tiers.len(), 2);
        assert_eq!(table.tiers[0].name, "high"); // highest priority first
        assert_eq!(table.tiers[0].priority, 80);
        assert_eq!(table.tiers[0].spin_period_us, Some(1000));
        assert_eq!(table.tiers[1].name, "low");
    }

    #[test]
    fn node_spanning_tiers_errors() {
        // v1: a node whose groups name different tiers is rejected.
        let mut s = sys("d");
        for t in ["high", "low"] {
            s.tiers.insert(
                t.to_string(),
                TierDef {
                    posix: Some(TierRtosSpec {
                        priority: if t == "high" { 80 } else { 10 },
                        stack_bytes: None,
                        preempt_threshold: None,
                        sched_class: None,
                    }),
                    ..Default::default()
                },
            );
        }
        let mut cbgs = BTreeMap::new();
        cbgs.insert(
            "control_node".to_string(),
            vec![cbg("ctrl", "high"), cbg("telem", "low")],
        );
        let err = resolve_tiers(&s, &cbgs, "posix").unwrap_err();
        assert!(matches!(err, TierResolveError::NodeSpansTiers { .. }));
    }

    #[test]
    fn node_override_moves_a_group_to_another_tier() {
        let mut s = sys("d");
        for t in ["high", "low"] {
            s.tiers.insert(
                t.to_string(),
                TierDef {
                    posix: Some(TierRtosSpec {
                        priority: if t == "high" { 80 } else { 10 },
                        stack_bytes: None,
                        preempt_threshold: None,
                        sched_class: None,
                    }),
                    ..Default::default()
                },
            );
        }
        s.node_overrides
            .push(super::super::cargo_metadata_schema::NodeOverride {
                name: "telem_node".to_string(),
                callback_groups: vec![super::super::cargo_metadata_schema::CallbackGroupOverride {
                    id: "telem".to_string(),
                    tier: "low".to_string(),
                }],
            });
        // telem_node declares telem in "high"; the override moves it to "low"
        // (each node stays pinned to one tier).
        let mut cbgs = BTreeMap::new();
        cbgs.insert("control_node".to_string(), vec![cbg("ctrl", "high")]);
        cbgs.insert("telem_node".to_string(), vec![cbg("telem", "high")]);
        let table = resolve_tiers(&s, &cbgs, "posix").unwrap();
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
        let err = resolve_tiers(&sys("d"), &cbgs, "posix").unwrap_err();
        assert!(matches!(err, TierResolveError::UnknownTier { .. }));
    }

    #[test]
    fn missing_rtos_spec_errors() {
        let mut s = sys("d");
        s.tiers.insert(
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
        let err = resolve_tiers(&s, &cbgs, "posix").unwrap_err();
        assert!(matches!(err, TierResolveError::MissingRtosSpec { .. }));
    }

    #[test]
    fn override_on_unknown_node_errors() {
        let mut s = sys("d");
        s.node_overrides
            .push(super::super::cargo_metadata_schema::NodeOverride {
                name: "ghost".to_string(),
                callback_groups: vec![],
            });
        let err = resolve_tiers(&s, &BTreeMap::new(), "posix").unwrap_err();
        assert!(matches!(err, TierResolveError::UnknownOverrideNode { .. }));
    }
}
