//! phase-521 W8 (b), RFC-0113 D9 — the generated PX4 entry.
//!
//! PX4 is a Tier 1 FRAMEWORK, not a board family: PX4 owns `main` and the
//! threads, and a tier's wait is a work item's schedule. So the entry is a PX4
//! module — `ModuleBase<>` boilerplate plus one `px4::ScheduledWorkItem`
//! subclass per tier — rather than a `Board::run_*` call. The work item's
//! runtime half is `integrations/px4/include/nros_px4/tier_work_item.hpp`.
//!
//! What is shared with the C++ entry, and what is not:
//!
//! * The NODES are the C++ entry's: the plan is admitted and lowered by the
//!   C++ pack (`admit_and_lower`), and the template renders each tier's setup
//!   with the same partials (`node_body_cpp.jinja`, `declare_calls.jinja`,
//!   `service_trailer_cpp.jinja`, the monitor statics and install). A component
//!   cannot tell which entry constructed it.
//! * The lowering runs against the `linux` family. PX4 SITL runs nano-ros on
//!   the POSIX platform (`NanoRosPx4Module.cmake` links the posix shim), and
//!   the family decides only the boot wrapper and the runners, neither of
//!   which this template renders.
//! * The WORK QUEUE of each tier comes from its rank:
//!   `nros_orchestration_ir::px4::px4_work_queues`, most urgent first, where a
//!   tier's urgency is its resolved priority (bigger is more urgent, the POSIX
//!   reading the `linux` lowering gives it). More tiers than PX4 has queues is
//!   refused here, at generation, never shared.
//! * A plan with no tier table is ONE tier, `default`, holding every node. A
//!   group-split plan (one node's groups on two tiers) is refused: its shape is
//!   a sched context per tier on ONE executor, and a work item per tier has no
//!   executor of its own to give it.

use nros_entry_lower::LoweredEntry;
use serde::Serialize;

use super::{ExecutorShape, Plan, emit::admit_and_lower, lower::LowerOptions, pack::manifests};

/// The board family the lowering runs against. See the module docs.
const LOWERING_BOARD: &str = "linux";

/// The pack whose admission and partials the PX4 entry shares.
const NODE_PACK: &str = "cpp";

/// The template, registered by the C++ pack's manifest.
const TEMPLATE: &str = "entry_px4.cpp";

/// One tier's work item.
#[derive(Debug, Serialize)]
struct Px4Tier {
    /// Position in the tier table, which is also the open order.
    index: usize,
    name: String,
    /// The PX4 work queue, a `px4::wq_configurations` member.
    queue: &'static str,
    /// 0 is the most urgent.
    rank: usize,
    /// Indices into `LoweredEntry::nodes`, in setup order.
    nodes: Vec<usize>,
    /// `(node name, node namespace, group)` triples; empty is the wildcard.
    groups: Vec<(String, String, String)>,
    /// Only the boot tier registers the process's param and lifecycle services.
    emits_services: bool,
    /// The boot tier carries the image's monitor table, as on every runner.
    monitors: Option<nros_entry_lower::MonitorTable>,
}

#[derive(Debug, Serialize)]
struct Px4View {
    /// The PX4 `MAIN` of the module: the shell command, and the
    /// `<main>_main` symbol PX4's module registration calls.
    main: String,
    tiers: Vec<Px4Tier>,
}

#[derive(Serialize)]
struct Px4Context<'a> {
    #[serde(flatten)]
    entry: &'a LoweredEntry,
    px4: Px4View,
}

/// Render `plan` as a PX4 module entry whose shell command is `main`.
pub fn emit_px4(plan: &Plan, main: &str, opts: &LowerOptions<'_>) -> Result<String, String> {
    if !is_c_ident(main) {
        return Err(format!(
            "PX4 entry: module main `{main}` is not a C identifier, and PX4 names the \
             module's entry symbol `{main}_main` from it"
        ));
    }
    let mut lowering = plan.clone();
    lowering.board = LOWERING_BOARD.into();
    if lowering.executor_shape() == ExecutorShape::SchedContexts {
        return Err(format!(
            "PX4 entry (`{main}`): a node's callback groups span two tiers, which runs \
             one executor with a sched context per tier; a PX4 tier is a work item with \
             its own executor, so each node's groups must sit on one tier (RFC-0113 D9)"
        ));
    }
    let all = manifests()?;
    let manifest = all
        .get(NODE_PACK)
        .ok_or_else(|| format!("no entry pack `{NODE_PACK}`"))?;
    let mut lowered = admit_and_lower(NODE_PACK, manifest, &all, &lowering, opts)?;
    lowered.board = plan.board.clone();

    let mut tiers: Vec<Px4Tier> = match &lowered.tiers {
        Some(t) => t
            .rows
            .iter()
            .zip(&t.setups)
            .map(|(row, setup)| Px4Tier {
                index: row.index,
                name: row.name.clone(),
                queue: "",
                rank: 0,
                nodes: setup.nodes.iter().map(|s| s.node).collect(),
                groups: row.groups.clone(),
                emits_services: setup.emits_services,
                monitors: setup.monitors.clone(),
            })
            .collect(),
        None => vec![Px4Tier {
            index: 0,
            name: "default".into(),
            queue: "",
            rank: 0,
            nodes: (0..lowered.nodes.len()).collect(),
            groups: Vec::new(),
            emits_services: true,
            monitors: lowered.monitors.clone(),
        }],
    };

    // Rank: most urgent first, by resolved priority, ties in table order.
    let priorities: Vec<i64> = match &lowered.tiers {
        Some(t) => t.rows.iter().map(|r| r.priority).collect(),
        None => vec![0],
    };
    let mut by_rank: Vec<usize> = (0..tiers.len()).collect();
    by_rank.sort_by_key(|&i| std::cmp::Reverse(priorities[i]));
    let names: Vec<&str> = by_rank.iter().map(|&i| tiers[i].name.as_str()).collect();
    let queues = nros_orchestration_ir::px4::px4_work_queues(&names)
        .map_err(|e| format!("PX4 entry (`{main}`): {e}"))?
        .into_iter()
        .map(|(_, q)| q)
        .collect::<Vec<_>>();
    for (rank, (&i, queue)) in by_rank.iter().zip(queues).enumerate() {
        tiers[i].rank = rank;
        tiers[i].queue = queue;
    }

    super::render::render(
        TEMPLATE,
        Px4Context {
            entry: &lowered,
            px4: Px4View {
                main: main.into(),
                tiers,
            },
        },
    )
}

fn is_c_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiered(n: usize) -> Plan {
        let mut p = super::super::golden::cpp_tiered_plan("px4");
        if let Some(t) = p.resolved_tiers.as_mut() {
            let proto = t.tiers[1].clone();
            for i in 2..n {
                let mut extra = proto.clone();
                extra.name = format!("extra{i}");
                extra.priority = 5 - i as i64;
                extra.members.clear();
                t.tiers.push(extra);
            }
        }
        p
    }

    /// RFC-0113 D9 -- each tier takes the queue its rank picks, and the rank
    /// is its priority, not its position in the table.
    #[test]
    fn each_tier_runs_on_the_queue_its_rank_picks() {
        let mut p = tiered(2);
        // List the less urgent tier FIRST: the queue must follow the priority.
        p.resolved_tiers.as_mut().unwrap().tiers.reverse();
        let src = emit_px4(&p, "nros_demo", &LowerOptions::default()).unwrap();
        let high = src
            .find("class NrosTier_high ")
            .expect("one work item class per tier");
        let low = src
            .find("class NrosTier_low ")
            .expect("one work item class per tier");
        let rate = src[high..]
            .find("px4::wq_configurations::rate_ctrl")
            .unwrap();
        let hp = src[low..]
            .find("px4::wq_configurations::hp_default")
            .unwrap();
        assert!(rate < src[high..].find('}').unwrap(), "{src}");
        assert!(hp < src[low..].find('}').unwrap(), "{src}");
        assert_eq!(src.matches(": public ::nros_px4::TierWorkItem").count(), 2);
    }

    /// More tiers than PX4 has queues is refused at generation, naming them.
    #[test]
    fn more_tiers_than_queues_is_refused() {
        let p = tiered(nros_orchestration_ir::px4::PX4_WORK_QUEUES.len() + 1);
        let err = emit_px4(&p, "nros_demo", &LowerOptions::default()).unwrap_err();
        assert!(err.contains("high") && err.contains("extra4"), "{err}");
    }

    /// A node whose groups span two tiers has no executor of its own per tier.
    #[test]
    fn a_group_split_plan_is_refused() {
        let mut p = tiered(2);
        p.nodes[0].group_tiers = std::collections::BTreeMap::from([
            ("ctrl_grp".to_string(), "high".to_string()),
            ("telem_grp".to_string(), "low".to_string()),
        ]);
        p.resolved_tiers.as_mut().unwrap().tiers[1]
            .members
            .push(("ctrl".into(), "telem_grp".into()));
        let err = emit_px4(&p, "nros_demo", &LowerOptions::default()).unwrap_err();
        assert!(err.contains("span two tiers"), "{err}");
    }

    #[test]
    fn a_main_that_is_not_an_identifier_is_refused() {
        let p = tiered(2);
        let err = emit_px4(&p, "nros-demo", &LowerOptions::default()).unwrap_err();
        assert!(err.contains("not a C identifier"), "{err}");
    }
}
