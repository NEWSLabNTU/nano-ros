//! phase-296 W5.2 — the RTOS realizer over the agnostic `RankedPlan`
//! (RFC-0052 §"nano-ros execution modeling" / play_launch phase-45 §45.10).
//!
//! The shared core ([`ros_launch_manifest_sched::chain_aware_rank`]) produced a
//! **priorityless** ordered/segmented ranking. This realizer turns it into a
//! concrete RTOS schedule for one board, mapping the six agnostic dimensions
//! (`activation, urgency, deadline, budget, non_preempt_scope, placement`) onto
//! the board's primitives — **preferring kernel-native features**, backfilling
//! with the nano-ros executor where a kernel lacks one, and **recording** any
//! degradation (fail-loud, the W2 rejection-table philosophy). It does NOT use
//! play_launch's `posix` realizer / `rt_priority_band` — per-platform guarantees
//! differ by design and on the record.
//!
//! v1 realizes the dims available from the model today: **urgency** (from the
//! ranking order), **activation** (Timer/Event from the path triggers),
//! **deadline** (`max_latency_ms`), and **budget** (`exec_ms`, when a path
//! carries a WCET). `non_preempt_scope` and `placement` are `NotRequested`
//! until the derivation supplies them (later waves).

use ros_launch_manifest_sched::{MapperInput, RankedPlan, chain::EffectiveTrigger};
use std::collections::BTreeMap;

/// A board's scheduling capabilities — what the realizer may target natively.
/// The `PlatformSched`/board seam (W5.3) supplies this per board; here it is a
/// plain descriptor the realizer reads.
#[derive(Clone, Debug, PartialEq)]
pub struct SchedCaps {
    /// Kernel earliest-deadline-first (Zephyr `CONFIG_SCHED_DEADLINE`, Linux
    /// `SCHED_DEADLINE`).
    pub edf: bool,
    /// Kernel execution-time reservation / sporadic server (NuttX
    /// `SCHED_SPORADIC`, Linux `SCHED_DEADLINE` runtime).
    pub reservation: bool,
    /// Native preemption-threshold (ThreadX).
    pub preempt_threshold: bool,
    /// SMP core affinity.
    pub affinity: bool,
    /// Number of distinct priority levels.
    pub n_priorities: u16,
    /// `true` when a numerically-lower priority is *higher* urgency
    /// (Zephyr/ThreadX); `false` when a higher number is higher urgency
    /// (FreeRTOS/POSIX/NuttX).
    pub low_number_is_high: bool,
}

/// How one requirement dimension was realized on this board.
#[derive(Clone, Debug, PartialEq)]
pub enum DimRealization {
    /// Honored by a kernel-native primitive.
    Native,
    /// Backfilled by the portable nano-ros executor (Sporadic `SchedContext`,
    /// EDF-among-callbacks, LET/TT window).
    Backfill,
    /// Degraded to an approximation — the guarantee changed. `reason` says how
    /// (surfaced fail-loud so the feasibility checker sees it).
    Degrade { reason: String },
    /// The dimension was absent for this node — nothing to realize.
    NotRequested,
}

/// One node's realized RTOS scheduling (the six dims), plus how each
/// non-trivial dim landed (the degradation record).
#[derive(Clone, Debug, PartialEq)]
pub struct RealizedNode {
    pub name: String,
    /// Board-direction-normalized priority (already flipped for
    /// `low_number_is_high`).
    pub priority: i64,
    /// Executor scheduling class: `"edf"` | `"sporadic"` | `"fifo"` |
    /// `"best_effort"`.
    pub sched_class: &'static str,
    /// Timer activation period, µs (periodic paths only).
    pub period_us: Option<u64>,
    pub deadline_us: Option<u64>,
    pub budget_us: Option<u64>,
    pub core: Option<u32>,
    pub preempt_threshold: Option<i64>,
    pub deadline_real: DimRealization,
    pub budget_real: DimRealization,
    pub preempt_real: DimRealization,
    pub placement_real: DimRealization,
}

/// A single recorded degradation (fail-loud): a `(node, dim)` whose guarantee
/// weakened on this board. Collected across the plan so the caller can warn /
/// reject.
#[derive(Clone, Debug, PartialEq)]
pub struct Degradation {
    pub node: String,
    pub dim: &'static str,
    pub reason: String,
}

/// The realizer output: one entry per ranked node plus the degradation record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RtosPlan {
    pub nodes: Vec<RealizedNode>,
    pub degradations: Vec<Degradation>,
}

/// Per-node facts distilled from the [`MapperInput`] (v1: activation +
/// deadline + budget; the ranking supplies urgency).
struct NodeFacts {
    /// The tightest declared latency budget (ms) over the node's paths — the
    /// deadline dimension.
    deadline_ms: Option<f64>,
    /// A declared execution-time budget (ms) over the node's paths, if any
    /// path carries a WCET (`exec_ms`) — the budget dimension.
    budget_ms: Option<f64>,
    /// Timer period (ms) when the node has a periodic path; `None` when the
    /// node is purely event-driven.
    period_ms: Option<f64>,
}

fn node_facts(input: &MapperInput) -> BTreeMap<&str, NodeFacts> {
    let mut out: BTreeMap<&str, NodeFacts> = BTreeMap::new();
    for node in &input.nodes {
        let mut deadline_ms: Option<f64> = None;
        let mut budget_ms: Option<f64> = None;
        let mut period_ms: Option<f64> = None;
        for p in &node.paths {
            if let Some(d) = p.max_latency_ms {
                deadline_ms = Some(deadline_ms.map_or(d, |cur: f64| cur.min(d)));
            }
            if let Some(b) = p.exec_ms {
                budget_ms = Some(budget_ms.map_or(b, |cur: f64| cur.max(b)));
            }
            if let EffectiveTrigger::Timer { rate_hz } = &p.effective_trigger {
                if *rate_hz > 0.0 {
                    let per = 1000.0 / rate_hz;
                    period_ms = Some(period_ms.map_or(per, |cur: f64| cur.min(per)));
                }
            }
        }
        out.insert(
            node.name.as_str(),
            NodeFacts {
                deadline_ms,
                budget_ms,
                period_ms,
            },
        );
    }
    out
}

/// Dense per-node rank from the ranking order: nodes sharing a `fine_group`
/// (segment) share a rank; a node's rank is the highest (lowest index) of its
/// items. Returns `name → dense_rank` (0 = most urgent) and the rank count.
fn dense_node_ranks(ranked: &RankedPlan) -> (BTreeMap<&str, usize>, usize) {
    let mut group_rank: BTreeMap<usize, usize> = BTreeMap::new();
    let mut next = 0usize;
    // First appearance of each fine_group defines its dense rank (order-
    // preserving; a simplification of the posix band-scarcity collapse —
    // adequate until a board's priority count is exceeded, a later refinement).
    for it in &ranked.items {
        group_rank.entry(it.fine_group).or_insert_with(|| {
            let r = next;
            next += 1;
            r
        });
    }
    let mut node_rank: BTreeMap<&str, usize> = BTreeMap::new();
    for it in &ranked.items {
        let r = group_rank[&it.fine_group];
        node_rank
            .entry(it.node.as_str())
            .and_modify(|cur| {
                if r < *cur {
                    *cur = r;
                }
            })
            .or_insert(r);
    }
    (node_rank, next.max(1))
}

/// Map a dense rank (0 = most urgent) to a board priority, honoring the count
/// and direction. Clamps into the band when ranks exceed `n_priorities`.
fn rank_to_priority(rank: usize, rank_count: usize, caps: &SchedCaps) -> i64 {
    let n = caps.n_priorities.max(1) as usize;
    // Compress dense ranks into [0, n): if there is room, 1:1; else clamp.
    let hi = rank_count.min(n).saturating_sub(1);
    let pos = rank.min(hi); // position from the top, 0 = most urgent
    if caps.low_number_is_high {
        pos as i64
    } else {
        (hi - pos) as i64
    }
}

/// Realize the agnostic ranking into an RTOS plan for a board.
pub fn realize_rtos(ranked: &RankedPlan, input: &MapperInput, caps: &SchedCaps) -> RtosPlan {
    let facts = node_facts(input);
    let (node_rank, rank_count) = dense_node_ranks(ranked);

    let mut nodes: Vec<RealizedNode> = Vec::new();
    let mut degradations: Vec<Degradation> = Vec::new();

    for (name, rank) in &node_rank {
        let f = facts.get(name);
        let priority = rank_to_priority(*rank, rank_count, caps);
        let period_us = f
            .and_then(|f| f.period_ms)
            .map(|ms| (ms * 1000.0).round().max(0.0) as u64);
        let deadline_ms = f.and_then(|f| f.deadline_ms);
        let budget_ms = f.and_then(|f| f.budget_ms);

        // deadline (dim): EDF native where the kernel has it; else the ranking
        // already encodes deadline-monotonic order — record the weakening.
        let (deadline_us, deadline_real, mut sched_class) = match deadline_ms {
            None => (None, DimRealization::NotRequested, "fifo"),
            Some(d) => {
                let us = (d * 1000.0).round().max(0.0) as u64;
                if caps.edf {
                    (Some(us), DimRealization::Native, "edf")
                } else {
                    let reason = "deadline realized as deadline-monotonic \
                                  priority (no kernel EDF)"
                        .to_string();
                    degradations.push(Degradation {
                        node: (*name).to_string(),
                        dim: "deadline",
                        reason: reason.clone(),
                    });
                    (Some(us), DimRealization::Degrade { reason }, "fifo")
                }
            }
        };

        // budget (dim): kernel reservation native; else executor Sporadic SC
        // backfill (portable). Never advisory-drop silently.
        let (budget_us, budget_real) = match budget_ms {
            None => (None, DimRealization::NotRequested),
            Some(b) => {
                let us = (b * 1000.0).round().max(0.0) as u64;
                if caps.reservation {
                    sched_class = "sporadic";
                    (Some(us), DimRealization::Native)
                } else {
                    sched_class = "sporadic"; // executor Sporadic SC backfill
                    (Some(us), DimRealization::Backfill)
                }
            }
        };

        // non_preempt_scope + placement: not derived from the model yet.
        let preempt_real = DimRealization::NotRequested;
        let placement_real = DimRealization::NotRequested;

        nodes.push(RealizedNode {
            name: (*name).to_string(),
            priority,
            sched_class,
            period_us,
            deadline_us,
            budget_us,
            core: None,
            preempt_threshold: None,
            deadline_real,
            budget_real,
            preempt_real,
            placement_real,
        });
    }

    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    RtosPlan {
        nodes,
        degradations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ros_launch_manifest_sched::{MapperNode, chain::MapperPath, chain_aware_rank, mapper::Criticality};

    fn caps(edf: bool, reservation: bool, low_high: bool) -> SchedCaps {
        SchedCaps {
            edf,
            reservation,
            preempt_threshold: false,
            affinity: false,
            n_priorities: 32,
            low_number_is_high: low_high,
        }
    }

    fn timer_path(name: &str, rate: f64, deadline: Option<f64>, exec: Option<f64>) -> MapperPath {
        MapperPath {
            name: name.to_string(),
            effective_trigger: EffectiveTrigger::Timer { rate_hz: rate },
            max_latency_ms: deadline,
            exec_ms: exec,
            inputs: vec![],
            outputs: vec![],
        }
    }

    fn input_two() -> MapperInput {
        MapperInput {
            nodes: vec![
                MapperNode {
                    name: "/hi".to_string(),
                    scope: "/".to_string(),
                    criticality: Some(Criticality::High),
                    paths: vec![timer_path("p", 50.0, Some(10.0), None)],
                    ..Default::default()
                },
                MapperNode {
                    name: "/lo".to_string(),
                    scope: "/".to_string(),
                    criticality: Some(Criticality::Low),
                    paths: vec![timer_path("p", 10.0, Some(80.0), None)],
                    ..Default::default()
                },
            ],
            legacy: None,
            chains: vec![],
        }
    }

    #[test]
    fn deadline_native_on_edf_board() {
        let input = input_two();
        let ranked = chain_aware_rank(&input);
        let plan = realize_rtos(&ranked, &input, &caps(true, false, false));

        let hi = plan.nodes.iter().find(|n| n.name == "/hi").unwrap();
        assert_eq!(hi.sched_class, "edf");
        assert_eq!(hi.deadline_real, DimRealization::Native);
        assert_eq!(hi.deadline_us, Some(10_000));
        assert!(plan.degradations.is_empty(), "EDF board: no deadline degrade");
        // 50 Hz → 20 ms period.
        assert_eq!(hi.period_us, Some(20_000));
    }

    #[test]
    fn deadline_degrades_recorded_without_edf() {
        let input = input_two();
        let ranked = chain_aware_rank(&input);
        let plan = realize_rtos(&ranked, &input, &caps(false, false, false));

        let hi = plan.nodes.iter().find(|n| n.name == "/hi").unwrap();
        assert_eq!(hi.sched_class, "fifo");
        assert!(matches!(hi.deadline_real, DimRealization::Degrade { .. }));
        // Fail-loud: the weakening is on the record.
        assert!(plan.degradations.iter().any(|d| d.node == "/hi" && d.dim == "deadline"));
    }

    #[test]
    fn budget_native_vs_backfill() {
        let mut input = input_two();
        input.nodes[0].paths[0].exec_ms = Some(3.0); // WCET on /hi

        let ranked = chain_aware_rank(&input);
        // Reservation board → native.
        let native = realize_rtos(&ranked, &input, &caps(true, true, false));
        let hi_n = native.nodes.iter().find(|n| n.name == "/hi").unwrap();
        assert_eq!(hi_n.sched_class, "sporadic");
        assert_eq!(hi_n.budget_us, Some(3_000));
        assert_eq!(hi_n.budget_real, DimRealization::Native);
        // No reservation → executor backfill (still sporadic, not dropped).
        let bf = realize_rtos(&ranked, &input, &caps(true, false, false));
        let hi_b = bf.nodes.iter().find(|n| n.name == "/hi").unwrap();
        assert_eq!(hi_b.budget_real, DimRealization::Backfill);
    }

    #[test]
    fn priority_reflects_rank_and_direction() {
        let input = input_two();
        let ranked = chain_aware_rank(&input);
        // High-number-is-high (POSIX/FreeRTOS): the more urgent /hi gets the
        // larger number.
        let hn = realize_rtos(&ranked, &input, &caps(false, false, false));
        let hi = hn.nodes.iter().find(|n| n.name == "/hi").unwrap();
        let lo = hn.nodes.iter().find(|n| n.name == "/lo").unwrap();
        assert!(hi.priority > lo.priority, "urgent node higher number");
        // Low-number-is-high (Zephyr/ThreadX): /hi gets the smaller number.
        let ln = realize_rtos(&ranked, &input, &caps(false, false, true));
        let hi2 = ln.nodes.iter().find(|n| n.name == "/hi").unwrap();
        let lo2 = ln.nodes.iter().find(|n| n.name == "/lo").unwrap();
        assert!(hi2.priority < lo2.priority, "urgent node lower number");
    }
}
