//! The record of an entry's thread priorities (RFC-0079 section 5: "the
//! result lands in a build artifact").
//!
//! `nros codegen entry` resolves a tier table, and on a derived or allocated
//! tier it computes the numbers itself, from the contract's ranks and the
//! image's priority plan. Until this module the only trace of that was one
//! stderr line, so nothing downstream could hold the image to the plan, and
//! code outside nano-ros that runs threads of its own (a supervisor, a
//! driver) had to restate the plan's numbers by hand: the authored-priority
//! failure RFC-0079 removes, one layer out.
//!
//! Two files, written beside the generated TU:
//!
//! * [`RECORD_JSON`] - the plan (direction, range, reserved bands, `pool.app`,
//!   where it came from), each tier (priority, how it was decided, members),
//!   each ranked node (priority, period, deadline) and the degradations. Read
//!   by `scripts/check-tier-priority-plan-image.py --derived-plan`.
//! * [`RECORD_HEADER`] - the same plan and tier priorities as C macros, so a
//!   thread the application creates is placed from the plan the tiers were
//!   allocated from.

use std::path::Path;

use eyre::{Result, WrapErr};
use nros_orchestration_ir::{
    DEFAULT_TIER, ResolvedTierTable,
    priority_plan::{Direction, PriorityPlan},
};
use serde_json::{Value, json};

use super::EntryDerivation;

/// The JSON record's file name, beside the generated TU.
pub const RECORD_JSON: &str = "nros_priority_plan.json";
/// The C header's file name, beside the generated TU.
pub const RECORD_HEADER: &str = "nros_priority_plan.h";

/// Everything the record is made of.
pub struct PriorityRecord<'a> {
    /// The tier key (`zephyr`, `freertos`, ...).
    pub tier_key: &'a str,
    /// The plan the tiers were allocated out of: the image's own, or the tier
    /// key's projection when the caller had no `.config`.
    pub plan: &'a PriorityPlan,
    /// True when `plan` was resolved from the image (`--dotconfig`).
    pub from_image: bool,
    /// The resolved tier table, `None` when the entry has no tiers.
    pub tiers: Option<&'a ResolvedTierTable>,
    /// What the derivation did, `None` when nothing was derived.
    pub derivation: Option<&'a EntryDerivation>,
    /// phase-509 P7 -- the entry's nodes (bare names, as the tier table names
    /// its members), for `NROS_ENTRY_LEVEL_OF_<node>`.
    pub nodes: &'a [&'a str],
    /// Issue 2019 -- the stacks the entry's nodes state (`system.toml
    /// [[component]] stack_bytes`), by bare name: each tier's largest member
    /// stack, which `mem-report --levels` derives the level's stack from.
    pub member_stacks: &'a [(&'a str, u32)],
}

fn direction_str(d: Direction) -> &'static str {
    match d {
        Direction::SmallerIsUrgent => "smaller-is-urgent",
        Direction::BiggerIsUrgent => "bigger-is-urgent",
    }
}

impl PriorityRecord<'_> {
    /// How the tier's priority was decided.
    fn origin(&self, tier: &str) -> &'static str {
        if let Some(d) = self.derivation {
            if d.derived_tiers.iter().any(|t| t == tier) {
                return "derived";
            }
            if d.allocated_tiers.iter().any(|t| t == tier) {
                return "allocated";
            }
        }
        if tier == DEFAULT_TIER {
            "default"
        } else {
            "authored"
        }
    }

    /// Does the tier host a node allocated above the transport?
    fn above_transport(&self, members: &[(String, String)]) -> bool {
        self.derivation.is_some_and(|d| {
            members.iter().any(|(n, _)| {
                d.above_transport
                    .iter()
                    .any(|f| f.rsplit('/').next() == Some(n.as_str()))
            })
        })
    }

    /// The tiers that run, `(name, priority)`, skipping the synthesised
    /// single `default` tier (an entry with it runs no tier at all).
    fn running_tiers(&self) -> Vec<(&str, i64)> {
        match self.tiers {
            Some(t) if !t.is_single_tier() => t
                .tiers
                .iter()
                .map(|r| (r.name.as_str(), r.priority))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// phase-509 P7 (RFC-0108 D8.1) -- each node's level, 0 the most urgent,
    /// and the level count. Until P9 a level is a tier: a running tier's
    /// level is its urgency order among the running tiers, equal priorities
    /// in table order, which is the order the runner's tier array has and
    /// the rule its `nros_cpp_tier_level` / `nros::tier_level_by` applies. An
    /// entry with no running tier is one level, every node at 0. A node no
    /// running tier holds has no level here.
    pub fn node_levels(&self) -> (u8, Vec<(String, u8)>) {
        let tiers = self.running_tiers();
        let smaller = self.plan.direction == Direction::SmallerIsUrgent;
        if tiers.is_empty() {
            return (1, self.nodes.iter().map(|n| (n.to_string(), 0)).collect());
        }
        let level_of = |i: usize| -> u8 {
            let mine = tiers[i].1;
            let ahead = tiers
                .iter()
                .enumerate()
                .filter(|&(j, &(_, q))| {
                    (if smaller { q < mine } else { q > mine }) || (q == mine && j < i)
                })
                .count();
            u8::try_from(ahead).unwrap_or(u8::MAX)
        };
        let mut out: Vec<(String, u8)> = Vec::new();
        if let Some(t) = self.tiers {
            for (i, r) in t.tiers.iter().enumerate() {
                for (node, _group) in &r.members {
                    let l = level_of(i);
                    match out.iter_mut().find(|(n, _)| n == node) {
                        // A node with groups on several tiers: its most
                        // urgent level.
                        Some((_, have)) => *have = (*have).min(l),
                        None => out.push((node.clone(), l)),
                    }
                }
            }
        }
        out.sort();
        (u8::try_from(tiers.len()).unwrap_or(u8::MAX), out)
    }

    /// The most urgent of the running tiers' priorities, in the plan's
    /// direction.
    pub fn most_urgent_tier(&self) -> Option<i64> {
        let ps = self.running_tiers().into_iter().map(|(_, p)| p);
        match self.plan.direction {
            Direction::SmallerIsUrgent => ps.min(),
            Direction::BiggerIsUrgent => ps.max(),
        }
    }

    /// The JSON record.
    pub fn to_json(&self) -> Value {
        let reserved: serde_json::Map<String, Value> = self
            .plan
            .reserved
            .iter()
            .map(|(k, b)| (k.clone(), json!([b.lo, b.hi])))
            .collect();
        let tiers: Vec<Value> = match self.tiers {
            Some(t) if !t.is_single_tier() => t
                .tiers
                .iter()
                .map(|r| {
                    json!({
                        "name": r.name,
                        "priority": r.priority,
                        "origin": self.origin(&r.name),
                        "above_transport": self.above_transport(&r.members),
                        "spin_period_us": r.spin_period_us,
                        // phase-509 P9 -- an authored `stack_bytes` (the
                        // expert override); `null` means the level's stack is
                        // derived from its members. The cost report
                        // (`mem-report --levels`) reads it.
                        "stack_bytes": r.stack_bytes,
                        // Issue 2019 -- the largest stack a member states
                        // (`[[component]] stack_bytes`), the `member` of
                        // `NROS_LEVEL_STACK_BYTES(member)`; 0 when none does.
                        "member_stack_bytes": r.members.iter()
                            .filter_map(|(n, _)| self.member_stacks.iter()
                                .find(|(m, _)| m == n)
                                .map(|(_, b)| *b))
                            .max()
                            .unwrap_or(0),
                        "members": r.members.iter()
                            .map(|(n, g)| json!([n, g]))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        let ranks: Vec<Value> = self
            .derivation
            .map(|d| {
                d.realized
                    .iter()
                    .map(|n| {
                        json!({
                            "node": n.name,
                            "priority": n.priority,
                            "period_us": n.period_us,
                            "deadline_us": n.deadline_us,
                            "sched_class": n.sched_class,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!({
            "version": 1,
            "tier_key": self.tier_key,
            "plan": {
                "direction": direction_str(self.plan.direction),
                "range": [self.plan.range.lo, self.plan.range.hi],
                "reserved": reserved,
                "app": [self.plan.app.lo, self.plan.app.hi],
                "source": self.plan.source,
                "from_image": self.from_image,
            },
            "tiers": tiers,
            "most_urgent_tier": self.most_urgent_tier(),
            "above_transport": self.derivation.map(|d| d.above_transport.clone()).unwrap_or_default(),
            "supervisor_priority": self.derivation.and_then(|d| d.supervisor_priority),
            "ranks": ranks,
            // phase-509 P5 (RFC-0108 D4): the preemption levels, one plan per
            // core (issue 0259) - the core, the cap and where it came from,
            // how the bands were chosen, and each level's priority, threshold
            // rank and members. Empty when nothing was derived.
            "levels": self
                .derivation
                .map(|d| serde_json::to_value(&d.levels).unwrap_or(Value::Null))
                .unwrap_or_else(|| Value::Array(Vec::new())),
            "degradations": self.derivation.map(|d| d.degradations.clone()).unwrap_or_default(),
            "shadowed": self.derivation.map(|d| d.shadowed.clone()).unwrap_or_default(),
        })
    }

    /// The C header.
    pub fn to_header(&self) -> String {
        let mut h = String::new();
        h.push_str(
            "/* Generated by `nros codegen entry` - DO NOT EDIT.\n * \
             The entry's priority plan and tier priorities (RFC-0079), the same\n * \
             numbers the generated entry runs its tiers at. Record: ",
        );
        h.push_str(RECORD_JSON);
        h.push_str(" */\n#ifndef NROS_PRIORITY_PLAN_H\n#define NROS_PRIORITY_PLAN_H\n\n");
        let mut define = |k: &str, v: i64| h.push_str(&format!("#define {k} ({v})\n"));
        define(
            "NROS_PRIORITY_PLAN_SMALLER_IS_URGENT",
            i64::from(self.plan.direction == Direction::SmallerIsUrgent),
        );
        define("NROS_PRIORITY_PLAN_FROM_IMAGE", i64::from(self.from_image));
        define("NROS_PRIORITY_PLAN_RANGE_LO", self.plan.range.lo);
        define("NROS_PRIORITY_PLAN_RANGE_HI", self.plan.range.hi);
        define("NROS_PRIORITY_PLAN_APP_LO", self.plan.app.lo);
        define("NROS_PRIORITY_PLAN_APP_HI", self.plan.app.hi);
        for (name, b) in &self.plan.reserved {
            let n = macro_name(name);
            define(&format!("NROS_PRIORITY_PLAN_RESERVED_{n}_LO"), b.lo);
            define(&format!("NROS_PRIORITY_PLAN_RESERVED_{n}_HI"), b.hi);
        }
        let tiers = self.running_tiers();
        define("NROS_ENTRY_TIER_COUNT", tiers.len() as i64);
        for (name, p) in &tiers {
            define(
                &format!("NROS_ENTRY_TIER_{}_PRIORITY", macro_name(name)),
                *p,
            );
        }
        if let Some(p) = self.most_urgent_tier() {
            define("NROS_ENTRY_TIERS_MOST_URGENT_PRIORITY", p);
        }
        // phase-509 P7 (RFC-0108 D8.1) -- which level hosts each node, the
        // level the runner places the node's executor at
        // (`nros_cpp_executor_level`).
        let (count, levels) = self.node_levels();
        define("NROS_ENTRY_LEVEL_COUNT", i64::from(count));
        for (node, l) in &levels {
            define(
                &format!("NROS_ENTRY_LEVEL_OF_{}", macro_name(node)),
                i64::from(*l),
            );
        }
        // RFC-0079 section 6: the priority for the supervisor of the
        // supervised deadlines, above the transport wherever the tiers sit.
        if let Some(p) = self.derivation.and_then(|d| d.supervisor_priority) {
            define("NROS_PRIORITY_PLAN_SUPERVISOR_PRIORITY", p);
        }
        h.push_str("\n#endif /* NROS_PRIORITY_PLAN_H */\n");
        h
    }

    /// The operator's view, one line per tier and per ranked node.
    pub fn summary_lines(&self) -> Vec<String> {
        let mut out = vec![format!(
            "priority plan ({}, {}): {} - {}",
            self.tier_key,
            direction_str(self.plan.direction),
            self.plan.describe_allocation(),
            if self.from_image {
                "the image's own"
            } else {
                "the tier key's projection"
            }
        )];
        if let Some(t) = self.tiers.filter(|t| !t.is_single_tier()) {
            for r in &t.tiers {
                out.push(format!(
                    "  tier {}: priority {} ({}{})",
                    r.name,
                    r.priority,
                    self.origin(&r.name),
                    if self.above_transport(&r.members) {
                        ", above the transport: the tier names it"
                    } else {
                        ""
                    }
                ));
            }
        }
        if let Some(p) = self.derivation.and_then(|d| d.supervisor_priority) {
            out.push(format!(
                "  supervisor of the supervised deadlines: priority {p}"
            ));
        }
        for l in self.derivation.map_or(&[][..], |d| d.levels.as_slice()) {
            out.push(format!(
                "  levels{}: {} of max_levels {} ({} cap), {}",
                l.core.map_or_else(String::new, |c| format!(" on core {c}")),
                l.levels.len(),
                l.max_levels,
                l.cap_source,
                l.method.as_str()
            ));
            for level in &l.levels {
                out.push(format!(
                    "  level {}: priority {}, threshold rank {}, members {}{}",
                    level.index,
                    level.priority,
                    level.threshold,
                    level
                        .members
                        .iter()
                        .map(|m| format!("{} (rank {})", m.node, m.rank))
                        .collect::<Vec<_>>()
                        .join(", "),
                    if level.tiers.is_empty() {
                        String::new()
                    } else {
                        format!(" - authored tier {}", level.tiers.join(", "))
                    }
                ));
            }
        }
        if let Some(d) = self.derivation {
            for n in &d.realized {
                out.push(format!(
                    "  rank {}: priority {}, period {}, deadline {}",
                    n.name,
                    n.priority,
                    n.period_us
                        .map_or_else(|| "-".to_string(), |v| format!("{v} us")),
                    n.deadline_us
                        .map_or_else(|| "-".to_string(), |v| format!("{v} us")),
                ));
            }
        }
        out
    }

    /// Write both files into `dir`.
    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).wrap_err_with(|| format!("create `{}`", dir.display()))?;
        let json_path = dir.join(RECORD_JSON);
        let text = serde_json::to_string_pretty(&self.to_json())? + "\n";
        write_if_changed(&json_path, &text)?;
        write_if_changed(&dir.join(RECORD_HEADER), &self.to_header())?;
        Ok(())
    }
}

/// Leave an unchanged file's mtime alone, so a reconfigure that derives the
/// same plan does not rebuild everything that includes the header.
fn write_if_changed(path: &Path, text: &str) -> Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    std::fs::write(path, text).wrap_err_with(|| format!("write `{}`", path.display()))
}

/// `pool.app` -> POOL_APP: upper case, every other character `_`.
fn macro_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_orchestration_ir::{
        ResolvedTier,
        priority_plan::Band,
        rtos_realizer::{DimRealization, RealizedNode},
    };

    fn island_plan() -> PriorityPlan {
        let mut reserved = std::collections::BTreeMap::new();
        reserved.insert("transport".to_string(), Band::new(4, 4));
        PriorityPlan {
            direction: Direction::SmallerIsUrgent,
            range: Band::new(-16, 14),
            reserved,
            app: Band::new(5, 14),
            source: "build/zephyr/.config".to_string(),
            max_levels: None,
        }
    }

    fn tier(name: &str, priority: i64) -> ResolvedTier {
        ResolvedTier {
            name: name.to_string(),
            priority,
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
            members: vec![("mrm_handler".to_string(), "main".to_string())],
        }
    }

    fn node(name: &str, priority: i64, period_us: u64) -> RealizedNode {
        RealizedNode {
            name: name.to_string(),
            priority,
            sched_class: "fifo",
            period_us: Some(period_us),
            deadline_us: None,
            budget_us: None,
            core: None,
            blocking_us: None,
            deadline_real: DimRealization::NotRequested,
            budget_real: DimRealization::NotRequested,
            placement_real: DimRealization::NotRequested,
        }
    }

    /// phase-509 P2 (RFC-0108 D2) -- the island's three ranks normalise to
    /// 0, 1 and 2 in urgency order whatever the kernel's direction; equal
    /// priorities share a rank.
    #[test]
    fn node_ranks_normalise_to_zero_most_urgent() {
        use nros_orchestration_ir::priority_plan::Direction;
        let derivation = EntryDerivation {
            realized: vec![
                node("/mrm_emergency_stop_operator", 7, 33_333),
                node("/mrm_comfortable_stop_operator", 8, 100_000),
                node("/mrm_handler", 5, 100_000),
                node("/twin", 7, 33_333),
            ],
            ..Default::default()
        };
        let ranks = |d| -> Vec<u8> {
            derivation
                .node_ranks(d)
                .into_iter()
                .map(|(_, r)| r)
                .collect()
        };
        assert_eq!(ranks(Direction::SmallerIsUrgent), vec![1, 2, 0, 1]);
        assert_eq!(ranks(Direction::BiggerIsUrgent), vec![1, 0, 2, 1]);
        assert_eq!(
            derivation.node_ranks(Direction::SmallerIsUrgent)[2].0,
            "/mrm_handler"
        );
    }

    /// One allocated tier on the island's plan: the record names its origin
    /// and the ranks it came from, the header the numbers the entry runs at.
    #[test]
    fn an_allocated_tier_is_recorded_with_its_plan_and_ranks() {
        let plan = island_plan();
        let table = ResolvedTierTable {
            tiers: vec![tier("mrm", 5)],
        };
        let derivation = EntryDerivation {
            allocated_tiers: vec!["mrm".to_string()],
            above_transport: vec!["/mrm_handler".to_string()],
            supervisor_priority: Some(4),
            realized: vec![
                node("/mrm_handler", 5, 100_000),
                node("/mrm_emergency_stop_operator", 7, 33_333),
            ],
            ..Default::default()
        };
        let rec = PriorityRecord {
            tier_key: "zephyr",
            plan: &plan,
            from_image: true,
            tiers: Some(&table),
            derivation: Some(&derivation),
            nodes: &["mrm_handler"],
            member_stacks: &[],
        };
        let j = rec.to_json();
        assert_eq!(j["plan"]["reserved"]["transport"], json!([4, 4]));
        assert_eq!(j["plan"]["app"], json!([5, 14]));
        assert_eq!(j["tiers"][0]["origin"], "allocated");
        assert_eq!(j["tiers"][0]["priority"], 5);
        assert_eq!(j["most_urgent_tier"], 5);
        assert_eq!(j["ranks"][1]["period_us"], 33_333);
        let h = rec.to_header();
        assert!(
            h.contains("#define NROS_PRIORITY_PLAN_RESERVED_TRANSPORT_LO (4)"),
            "{h}"
        );
        assert!(h.contains("#define NROS_PRIORITY_PLAN_APP_LO (5)"), "{h}");
        assert!(h.contains("#define NROS_ENTRY_TIER_COUNT (1)"), "{h}");
        assert!(
            h.contains("#define NROS_ENTRY_TIER_MRM_PRIORITY (5)"),
            "{h}"
        );
        assert!(
            h.contains("#define NROS_ENTRY_TIERS_MOST_URGENT_PRIORITY (5)"),
            "{h}"
        );
        assert_eq!(j["tiers"][0]["above_transport"], true);
        assert_eq!(j["supervisor_priority"], 4);
        assert_eq!(j["levels"], json!([]), "nothing banded in this fixture");
        assert!(
            h.contains("#define NROS_PRIORITY_PLAN_SUPERVISOR_PRIORITY (4)"),
            "{h}"
        );
        // phase-509 P7: one tier is one level, and it hosts the handler.
        assert!(h.contains("#define NROS_ENTRY_LEVEL_COUNT (1)"), "{h}");
        assert!(
            h.contains("#define NROS_ENTRY_LEVEL_OF_MRM_HANDLER (0)"),
            "{h}"
        );
    }

    /// phase-509 P7 (RFC-0108 D8.1) -- one `NROS_ENTRY_LEVEL_OF_<node>` per
    /// node: a running tier's level is its urgency order (Zephyr, smaller is
    /// urgent), whatever its place in the table; an entry with no running
    /// tier is one level with every node at 0.
    #[test]
    fn the_header_names_each_nodes_level() {
        let plan = island_plan();
        let mut handler = tier("hi", 5);
        handler.members = vec![("mrm_handler".to_string(), "main".to_string())];
        let mut ops = tier("lo", 7);
        ops.members = vec![
            (
                "mrm_emergency_stop_operator".to_string(),
                "main".to_string(),
            ),
            (
                "mrm_comfortable_stop_operator".to_string(),
                "main".to_string(),
            ),
        ];
        // Table order is raw priority descending: the less urgent tier first.
        let table = ResolvedTierTable {
            tiers: vec![ops, handler],
        };
        let rec = PriorityRecord {
            tier_key: "zephyr",
            plan: &plan,
            from_image: true,
            tiers: Some(&table),
            derivation: None,
            nodes: &[
                "mrm_handler",
                "mrm_emergency_stop_operator",
                "mrm_comfortable_stop_operator",
            ],
            member_stacks: &[],
        };
        let h = rec.to_header();
        assert!(h.contains("#define NROS_ENTRY_LEVEL_COUNT (2)"), "{h}");
        assert!(
            h.contains("#define NROS_ENTRY_LEVEL_OF_MRM_HANDLER (0)"),
            "{h}"
        );
        assert!(
            h.contains("#define NROS_ENTRY_LEVEL_OF_MRM_EMERGENCY_STOP_OPERATOR (1)"),
            "{h}"
        );
        assert!(
            h.contains("#define NROS_ENTRY_LEVEL_OF_MRM_COMFORTABLE_STOP_OPERATOR (1)"),
            "{h}"
        );
        // Issue 2019 -- each tier carries its largest stated member stack,
        // what `mem-report --levels` derives the level's stack from.
        let stated = PriorityRecord {
            member_stacks: &[
                ("mrm_emergency_stop_operator", 3000),
                ("mrm_comfortable_stop_operator", 5000),
            ],
            ..rec
        };
        let j = stated.to_json();
        let members: Vec<u64> = j["tiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["member_stack_bytes"].as_u64().unwrap())
            .collect();
        assert_eq!(members, [5000, 0], "{j}");
        let tierless = PriorityRecord {
            tiers: None,
            ..stated
        };
        let h = tierless.to_header();
        assert!(h.contains("#define NROS_ENTRY_LEVEL_COUNT (1)"), "{h}");
        assert!(
            h.contains("#define NROS_ENTRY_LEVEL_OF_MRM_EMERGENCY_STOP_OPERATOR (0)"),
            "{h}"
        );
    }

    /// phase-509 P5 (RFC-0108 D4) -- the record carries the levels: the
    /// island's three ranks under a cap of 2 are {handler} | {both
    /// operators}, and each level names its members and their ranks.
    #[test]
    fn the_record_carries_the_levels() {
        use nros_orchestration_ir::levels::{LevelInput, LevelNode, derive_levels};
        let free = [
            LevelNode {
                node: "/mrm_handler".to_string(),
                priority: 5,
                protected: Some("supervised"),
            },
            LevelNode {
                node: "/mrm_emergency_stop_operator".to_string(),
                priority: 7,
                protected: None,
            },
            LevelNode {
                node: "/mrm_comfortable_stop_operator".to_string(),
                priority: 8,
                protected: None,
            },
        ];
        let levels = derive_levels(&LevelInput {
            core: None,
            floating_interference: false,
            free: &free,
            pinned: &[],
            tasks: &[],
            max_levels: 2,
            cap_source: "image",
            low_number_is_high: true,
            time_slicing: false,
            ranked_priorities: &[],
        });
        let derivation = EntryDerivation {
            levels: vec![levels],
            ..Default::default()
        };
        let plan = island_plan();
        let rec = PriorityRecord {
            tier_key: "zephyr",
            plan: &plan,
            from_image: true,
            tiers: None,
            derivation: Some(&derivation),
            nodes: &[],
            member_stacks: &[],
        };
        let j = rec.to_json();
        assert_eq!(j["levels"][0]["max_levels"], 2);
        assert_eq!(j["levels"][0]["cap_source"], "image");
        assert_eq!(j["levels"][0]["method"], "capped");
        assert_eq!(
            j["levels"][0]["levels"][0]["members"][0]["node"],
            "/mrm_handler"
        );
        assert_eq!(j["levels"][0]["levels"][1]["priority"], 7);
        assert_eq!(j["levels"][0]["levels"][1]["threshold"], 1);
        assert_eq!(
            j["levels"][0]["levels"][1]["members"][1]["node"],
            "/mrm_comfortable_stop_operator"
        );
        assert_eq!(j["levels"][0]["levels"][1]["members"][1]["rank"], 2);
        let lines = rec.summary_lines();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("levels: 2 of max_levels 2")),
            "{lines:?}"
        );
    }

    /// No tiers (or only the synthesised `default` one): the plan is still
    /// recorded, the tier count is 0 and there is no most-urgent macro for a
    /// caller to mistake for a tier.
    #[test]
    fn a_tierless_entry_records_the_plan_and_no_tier() {
        let plan = island_plan();
        let table = ResolvedTierTable {
            tiers: vec![tier(DEFAULT_TIER, 0)],
        };
        let rec = PriorityRecord {
            tier_key: "zephyr",
            plan: &plan,
            from_image: true,
            tiers: Some(&table),
            derivation: None,
            nodes: &[],
            member_stacks: &[],
        };
        assert_eq!(rec.to_json()["tiers"], json!([]));
        let h = rec.to_header();
        assert!(h.contains("#define NROS_ENTRY_TIER_COUNT (0)"), "{h}");
        assert!(!h.contains("MOST_URGENT"), "{h}");
    }
}
