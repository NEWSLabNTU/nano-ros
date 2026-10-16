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
                        "spin_period_us": r.spin_period_us,
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
            "ranks": ranks,
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
        for (name, p) in self.running_tiers() {
            out.push(format!(
                "  tier {name}: priority {p} ({})",
                self.origin(name)
            ));
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

/// `pool.app` -> `POOL_APP`: upper case, every other character `_`.
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
            preempt_threshold: None,
            blocking_us: None,
            deadline_real: DimRealization::NotRequested,
            budget_real: DimRealization::NotRequested,
            preempt_real: DimRealization::NotRequested,
            placement_real: DimRealization::NotRequested,
        }
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
        };
        assert_eq!(rec.to_json()["tiers"], json!([]));
        let h = rec.to_header();
        assert!(h.contains("#define NROS_ENTRY_TIER_COUNT (0)"), "{h}");
        assert!(!h.contains("MOST_URGENT"), "{h}");
    }
}
