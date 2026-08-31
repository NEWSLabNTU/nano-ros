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

use ros_launch_manifest_sched::{
    MapperInput, RankedPlan, ResolvedTier, ResolvedTierTable, chain::EffectiveTrigger,
};
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
    /// Issue 0259 — how many cores this image runs on, when the deployment says.
    ///
    /// `None` is the default and means UNKNOWN, not one. Nothing in the board
    /// descriptors records a core count, and a bake cannot infer one: assuming
    /// 1 would report false over-subscription on an 8-core host, and assuming
    /// many would excuse a taskset that cannot fit. Both are fabricated
    /// hardware, which is the same failure as a fabricated WCET.
    ///
    /// Set it per deployment with `[deploy.<board>] cores = <n>`, the same
    /// bake-authoritative knob shape as `edf`. Until something declares it, the
    /// utilisation check stays silent and `placement` remains underivable.
    pub n_cores: Option<u16>,
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
    /// Issue 0259 — the derived BLOCKING term `B_i`, µs: the longest a callback
    /// on this node can be made to wait for a mutually-exclusive sibling.
    ///
    /// Derived, never authored. `None` means "not derivable" — fewer than two
    /// callbacks carry a WCET — and specifically NOT "no blocking". A
    /// feasibility check that reads `None` as zero is the optimism issue 0259
    /// is about; one that reads it as unknown is correct.
    pub blocking_us: Option<u64>,
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

/// The board seam (W5.3): the per-platform [`SchedCaps`] the realizer targets.
/// Grounded in the platform scheduler survey (RFC-0052 §"Scheduling model
/// evolution"). `target` is the RTOS name (`posix`/`native`/`freertos`/
/// `zephyr`/`threadx`/`nuttx`/`nuttx-riscv`/bare-metal aliases), normalized like
/// `nros_orchestration_ir`'s board routing. Kept consistent with W2's
/// applicability table (e.g. preemption-threshold is ThreadX-only).
///
/// `n_priorities` is a conservative platform default; a board descriptor can
/// refine it (Kconfig `CONFIG_NUM_PREEMPT_PRIORITIES` / `configMAX_PRIORITIES`
/// / `TX_MAX_PRIORITIES`) — a follow-up.
pub fn sched_caps_for(target: &str) -> SchedCaps {
    let t = target.trim().to_ascii_lowercase();
    let fam = t.as_str();
    match fam {
        // Linux / POSIX host — phase-302 W1 (issue 0261): caps describe
        // what nano-ros DELIVERS, not what Linux could do. edf/reservation
        // are FALSE — no sched_setattr/SCHED_DEADLINE consumer exists
        // (priorities are documented-advisory too); flip when phase-162
        // lands real consumers. affinity stays TRUE: 296-W5.13 landed the
        // sched_setaffinity tier consumer (runtime kernel-accept proven).
        //
        // That affinity consumer is `nros-board-linux`, and it is LINUX, not
        // POSIX: `sched_setaffinity` / `cpu_set_t` / `CPU_SET` are absent from
        // libc's apple module. Both keys land in this arm because the only
        // hosted board in the tree is the Linux one — on a non-Linux POSIX host
        // the affinity cap would over-promise.
        "posix" | "native" => SchedCaps {
            edf: false,
            reservation: false,
            preempt_threshold: false,
            affinity: true,
            n_priorities: 99,
            n_cores: None,

            low_number_is_high: false,
        },
        // Zephyr: CONFIG_SCHED_DEADLINE (EDF), SMP cpu_mask; no reservation /
        // preemption-threshold (cooperative priorities instead); low = high.
        "zephyr" => SchedCaps {
            edf: true,
            reservation: false,
            preempt_threshold: false,
            affinity: true,
            n_priorities: 32,
            n_cores: None,

            low_number_is_high: true,
        },
        // FreeRTOS: fixed-priority only; SMP core affinity; high = high.
        f if f.contains("freertos") => SchedCaps {
            edf: false,
            reservation: false,
            preempt_threshold: false,
            affinity: true,
            n_priorities: 16,
            n_cores: None,

            low_number_is_high: false,
        },
        // ThreadX: native preemption-threshold; SMP core exclude
        // (296-W5.13 consumer, fail-loud on non-SMP ports); low = high.
        f if f.contains("threadx") => SchedCaps {
            edf: false,
            reservation: false,
            preempt_threshold: true,
            affinity: true,
            n_priorities: 32,
            n_cores: None,

            low_number_is_high: true,
        },
        // NuttX: POSIX SCHED_SPORADIC (reservation); SMP affinity; high = high.
        f if f.contains("nuttx") => SchedCaps {
            edf: false,
            reservation: true,
            preempt_threshold: false,
            affinity: true,
            n_priorities: 255,
            n_cores: None,

            low_number_is_high: false,
        },
        // Bare-metal (RTIC / Cortex-M NVIC): hardware priorities, single core,
        // SRP ceiling (not a preemption-threshold knob); high = high (RTIC).
        _ => SchedCaps {
            edf: false,
            reservation: false,
            preempt_threshold: false,
            affinity: false,
            n_priorities: 8,
            n_cores: None,

            low_number_is_high: false,
        },
    }
}

/// [`SchedCaps`] for a target, with the per-deploy `edf` capability knob
/// applied. The knob is the bake-authoritative SSoT (RFC-0052 §"CAPS
/// provenance"): a `[deploy.<board>] edf = <bool>` in the deploy config
/// (carried on `Deploy.extra`) OVERRIDES the platform default, so the
/// realizer's `Native`-vs-`Degrade` decision is accurate against the image
/// that will actually be built. Absent knob → the platform default stands.
pub fn sched_caps_from_deploy(
    target: &str,
    deploy: Option<&ros_launch_manifest_model::Deploy>,
) -> SchedCaps {
    let mut caps = sched_caps_for(target);
    if let Some(d) = deploy
        && let Some(ros_launch_manifest_model::ExtraValue::Bool(b)) = d.extra.get("edf")
    {
        caps.edf = *b;
    }
    // Issue 0259 — `[deploy.<board>] cores = <n>`. Same knob shape as `edf`:
    // the deployment is the only place that knows, and it is authoritative for
    // the image actually being built. A non-positive count is ignored rather
    // than trusted — zero cores is not a claim, it is a typo, and honouring it
    // would divide by nothing.
    if let Some(d) = deploy
        && let Some(ros_launch_manifest_model::ExtraValue::Int(n)) = d.extra.get("cores")
        && *n > 0
    {
        caps.n_cores = Some((*n).min(u16::MAX as i64) as u16);
    }
    caps
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
    /// Issue 0259 — the BLOCKING term `B_i`: the longest WCET among this node's
    /// OTHER callbacks.
    ///
    /// A node's callbacks are mutually exclusive within their tier task (that
    /// is what `CallbackGroupDecl { type: "MutuallyExclusive" }` means, and v1
    /// treats every group that way), so a callback that becomes ready while a
    /// sibling is executing waits for that sibling to finish. The longest
    /// sibling is the worst case, and it is exactly the term a feasibility
    /// check must add to a response time.
    ///
    /// `None` when the node has fewer than two callbacks carrying a WCET —
    /// there is then no sibling to wait for, or no measurement of one. NOT
    /// zero: absent blocking and zero blocking are different claims, and
    /// issue 0259 is what happens when they are conflated.
    blocking_ms: Option<f64>,
}

fn node_facts(input: &MapperInput) -> BTreeMap<&str, NodeFacts> {
    let mut out: BTreeMap<&str, NodeFacts> = BTreeMap::new();
    for node in &input.nodes {
        let mut deadline_ms: Option<f64> = None;
        let mut budget_ms: Option<f64> = None;
        let mut period_ms: Option<f64> = None;
        // Every declared WCET on this node, so the blocking term can name the
        // longest one that is NOT the callback being blocked. Collected rather
        // than folded because `B_i` is a max over a set with one element
        // removed, which a running max cannot express.
        let mut execs: Vec<f64> = Vec::new();
        for p in &node.paths {
            if let Some(d) = p.max_latency_ms {
                deadline_ms = Some(deadline_ms.map_or(d, |cur: f64| cur.min(d)));
            }
            if let Some(b) = p.exec_ms {
                budget_ms = Some(budget_ms.map_or(b, |cur: f64| cur.max(b)));
                execs.push(b);
            }
            if let EffectiveTrigger::Timer { rate_hz } = &p.effective_trigger
                && *rate_hz > 0.0
            {
                let per = 1000.0 / rate_hz;
                period_ms = Some(period_ms.map_or(per, |cur: f64| cur.min(per)));
            }
        }
        // `B_i` for the WORST-placed callback on this node: the longest
        // sibling any one of them can be made to wait for. With every callback
        // in one mutually-exclusive group, that is the longest WCET among the
        // others — i.e. the second-largest overall, since the largest cannot
        // block itself.
        execs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let blocking_ms = execs.get(1).copied();

        out.insert(
            node.name.as_str(),
            NodeFacts {
                deadline_ms,
                budget_ms,
                period_ms,
                blocking_ms,
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

        // Issue 0259 — the blocking term IS derivable now that callbacks can
        // carry WCETs (RFC-0078). `B_i` is the longest mutually-exclusive
        // sibling; the executor serialises a node's callbacks within their tier
        // task, so a ready callback waits for whichever sibling is running.
        let blocking_us = f
            .and_then(|f| f.blocking_ms)
            .map(|ms| (ms * 1000.0).round().max(0.0) as u64);

        // non_preempt_scope + placement: still not derived, and deriving the
        // preemption THRESHOLD here would be a tautology rather than progress.
        // Issue 0259's rule is `ceiling = max urgency among the group's
        // members`; `dense_node_ranks` already gives a node the MINIMUM (best)
        // rank among its own paths, so that ceiling is the node's own priority
        // by construction and `preempt_threshold == priority` would say
        // nothing. A threshold earns its keep only when callbacks inside ONE
        // task hold DIFFERENT priorities, which this plan does not yet model —
        // recorded in 0259 rather than papered over with a derived no-op.
        let preempt_real = DimRealization::NotRequested;
        let placement_real = DimRealization::NotRequested;

        // Issue 0259 — the first CONSUMER of `B_i`: a necessary-condition
        // schedulability check.
        //
        // If a node's own worst callback plus the longest sibling it can wait
        // for already exceeds its deadline, no priority assignment, core pin or
        // preemption threshold can rescue it — interference from other nodes
        // only adds. So this reports a fact about the declaration rather than
        // about the schedule, which is why it is sound without a taskset model.
        //
        // Deliberately NOT full response-time analysis. RTA needs
        // `Σ over higher-priority tasks ceil(R/T)*C`, and with most callbacks
        // carrying no WCET that sum would be missing terms — an optimistic
        // number presented as an upper bound, which is the exact shape of
        // issue 0259. A necessary condition can only ever MISS an infeasible
        // node; it cannot invent one.
        //
        // `B` unknown is treated as 0 HERE and only here: it biases toward
        // silence, so the check stays free of false alarms. That is the safe
        // direction for something that stops a build; the unsafe direction is
        // the one `blocking_us: None` refuses everywhere else.
        if let (Some(c_us), Some(d_us)) = (budget_us, deadline_us) {
            let b_us = blocking_us.unwrap_or(0);
            if c_us.saturating_add(b_us) > d_us {
                let b_shown = match blocking_us {
                    Some(b) => format!("{b}us"),
                    None => "unknown (counted as 0)".to_string(),
                };
                degradations.push(Degradation {
                    node: (*name).to_string(),
                    dim: "feasibility",
                    reason: format!(
                        "execution + blocking exceeds the deadline before any \
                         interference is considered: C={c_us}us + B={b_shown} > \
                         D={d_us}us. No priority, core pin or preemption \
                         threshold can recover this; the declaration itself is \
                         infeasible (issue 0259)."
                    ),
                });
            }
        }

        nodes.push(RealizedNode {
            name: (*name).to_string(),
            priority,
            sched_class,
            period_us,
            deadline_us,
            budget_us,
            core: None,
            preempt_threshold: None,
            blocking_us,
            deadline_real,
            budget_real,
            preempt_real,
            placement_real,
        });
    }

    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    // Issue 0259 — system-level utilisation, the other quantitative term W1's
    // WCETs unlock.
    //
    // `U_i = C_i / T_i` is the fraction of a processor a periodic task needs.
    // Summed, it is a NECESSARY condition in the same family as the per-node
    // check above: a taskset demanding more than one processor cannot run on
    // one, whatever the priorities. Reported only when it EXCEEDS capacity,
    // because a utilisation that fits says nothing on its own — Liu & Layland's
    // bound is `n(2^(1/n)-1)` for rate-monotonic, and this makes no claim about
    // schedulability BELOW 1.0.
    //
    // The denominator is `caps.n_cores`, declared by
    // `[deploy.<board>] cores = <n>`. Unknown means SILENT: nothing in the
    // board descriptors records a count, and a bake cannot infer one —
    // assuming 1 would report false over-subscription on an 8-core host, and
    // assuming many would excuse a taskset that cannot fit.
    //
    // This gate was `!caps.affinity` when the check landed, which was a bug:
    // `affinity` is `true` for posix, zephyr, freertos, threadx and nuttx — every
    // real target — so the check never fired anywhere. A capability flag was
    // standing in for a quantity it cannot express.
    //
    // Nodes with an unknown C or T contribute nothing and are NAMED in the
    // message, so a total under capacity cannot be mistaken for "the system
    // fits" when half the taskset was unmeasured.
    if let Some(cores) = caps.n_cores {
        let mut total = 0.0_f64;
        let mut counted = 0usize;
        let mut unmeasured: Vec<&str> = Vec::new();
        for n in &nodes {
            match (n.budget_us, n.period_us) {
                (Some(c), Some(t)) if t > 0 => {
                    total += (c as f64) / (t as f64);
                    counted += 1;
                }
                _ => unmeasured.push(n.name.as_str()),
            }
        }
        let capacity = cores as f64;
        if counted > 0 && total > capacity {
            let caveat = if unmeasured.is_empty() {
                String::new()
            } else {
                format!(
                    " — and {} node(s) contributed NOTHING for want of a WCET or a \
                     period ({}), so the real total is higher",
                    unmeasured.len(),
                    unmeasured.join(", ")
                )
            };
            degradations.push(Degradation {
                node: "<system>".to_string(),
                dim: "utilization",
                reason: format!(
                    "the {counted} measured periodic node(s) demand {:.2} processors, \
                     and this deployment declares {cores}{caveat}. No priority \
                     assignment can run a taskset that does not fit (issue 0259).",
                    total
                ),
            });
        }
    }

    RtosPlan {
        nodes,
        degradations,
    }
}

/// W5.4 — wire the realization into the existing bake: convert an [`RtosPlan`]
/// into the [`ResolvedTierTable`] the `codegen-system` plan emitter + the
/// `run_tiers` const table already consume (one tier per realized node; the
/// executor lowers `class`/`period_us`/`budget_us`/`deadline_us` into its
/// `SchedContext` — Sporadic budget / EDF / TT — per W3a). `low_number_is_high`
/// (from [`SchedCaps`]) is needed to order the table by URGENCY (the table is
/// "most urgent first"; the realized priority is already board-direction-
/// normalized, so the numeric sort flips with the platform).
pub fn rtos_plan_to_tier_table(plan: &RtosPlan, low_number_is_high: bool) -> ResolvedTierTable {
    let mut tiers: Vec<ResolvedTier> = plan
        .nodes
        .iter()
        .map(|n| {
            // A node carrying a deadline/budget is real-time; otherwise a plain
            // fixed-priority best-effort tier (mirrors W3a's class lowering).
            let class = if n.deadline_us.is_some() || n.budget_us.is_some() {
                "real_time"
            } else {
                "best_effort"
            };
            ResolvedTier {
                name: n.name.clone(),
                priority: n.priority,
                // rlm v0.1.5 added the typed POSIX placement. The RTOS realizer
                // lowers to an RTOS tier table, so there is no host placement to
                // carry — `None` is the accurate answer, not a placeholder.
                posix: None,
                sched_class: Some(n.sched_class.to_string()),
                class: Some(class.to_string()),
                period_us: n.period_us,
                budget_us: n.budget_us,
                deadline_us: n.deadline_us,
                core: n.core,
                preempt_threshold: n.preempt_threshold,
                members: vec![n.name.clone()],
                stack_bytes: None,
                spin_period_us: None,
                deadline_policy: None,
            }
        })
        .collect();
    // Most-urgent-first. Urgency = smaller priority number when low=high, else
    // larger. Name breaks ties (deterministic).
    tiers.sort_by(|a, b| {
        let urgency = if low_number_is_high {
            a.priority.cmp(&b.priority)
        } else {
            b.priority.cmp(&a.priority)
        };
        urgency.then_with(|| a.name.cmp(&b.name))
    });
    ResolvedTierTable { tiers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ros_launch_manifest_sched::{
        MapperNode, chain::MapperPath, chain_aware_rank, mapper::Criticality,
    };

    fn caps(edf: bool, reservation: bool, low_high: bool) -> SchedCaps {
        SchedCaps {
            n_cores: None,
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
            // Undeclared, matching what `mapper_input` emits — these tests are
            // about the deadline/budget dimensions, and a fabricated jitter or
            // miss tolerance here would exercise a shape nano-ros never builds.
            max_jitter_ms: None,
            miss: None,
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
        assert!(
            plan.degradations.is_empty(),
            "EDF board: no deadline degrade"
        );
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
        assert!(
            plan.degradations
                .iter()
                .any(|d| d.node == "/hi" && d.dim == "deadline")
        );
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
    fn sched_caps_per_platform() {
        assert!(sched_caps_for("zephyr").edf);
        assert!(!sched_caps_for("zephyr").reservation);
        assert!(sched_caps_for("zephyr").low_number_is_high);
        assert!(!sched_caps_for("qemu-arm-freertos").edf);
        assert!(sched_caps_for("threadx-linux").preempt_threshold);
        assert!(sched_caps_for("threadx-linux").low_number_is_high);
        assert!(sched_caps_for("nuttx").reservation);
        assert!(!sched_caps_for("nuttx").low_number_is_high);
        // phase-302 W1 (issue 0261): posix caps describe what nano-ros
        // DELIVERS — nothing native on posix until phase-162 consumers land.
        let posix = sched_caps_for("native");
        assert!(!posix.edf && !posix.reservation && !posix.low_number_is_high);
        // 296-W5.13 consumers: affinity is genuinely delivered on both.
        assert!(posix.affinity);
        assert!(sched_caps_for("threadx-linux").affinity);
        // Unknown → bare-metal defaults (single-core, no EDF).
        assert!(!sched_caps_for("stm32f4-rtic").affinity);
    }

    #[test]
    fn same_ranking_realizes_differently_per_platform() {
        // W5.3 done-when: one ranking, two platforms, guarantee difference
        // recorded — Zephyr honors the deadline natively (EDF); FreeRTOS
        // degrades it to deadline-monotonic priority.
        let input = input_two();
        let ranked = chain_aware_rank(&input);

        let zephyr = realize_rtos(&ranked, &input, &sched_caps_for("zephyr"));
        let hi_z = zephyr.nodes.iter().find(|n| n.name == "/hi").unwrap();
        assert_eq!(hi_z.sched_class, "edf");
        assert_eq!(hi_z.deadline_real, DimRealization::Native);
        assert!(zephyr.degradations.is_empty());

        let freertos = realize_rtos(&ranked, &input, &sched_caps_for("freertos"));
        let hi_f = freertos.nodes.iter().find(|n| n.name == "/hi").unwrap();
        assert_eq!(hi_f.sched_class, "fifo");
        assert!(matches!(hi_f.deadline_real, DimRealization::Degrade { .. }));
        assert!(!freertos.degradations.is_empty());
    }

    #[test]
    fn wires_into_tier_table() {
        // W5.4: RtosPlan → ResolvedTierTable the existing bake consumes. Six-dim
        // fields ride through; ordering is most-urgent-first per direction.
        let input = input_two();
        let ranked = chain_aware_rank(&input);
        let caps = sched_caps_for("zephyr"); // edf, low_number_is_high
        let plan = realize_rtos(&ranked, &input, &caps);
        let table = rtos_plan_to_tier_table(&plan, caps.low_number_is_high);

        assert_eq!(table.tiers.len(), 2, "one tier per node");
        // /hi is most urgent → first (zephyr low=high → smaller number first).
        let hi = &table.tiers[0];
        assert_eq!(hi.name, "/hi");
        assert_eq!(hi.members, vec!["/hi".to_string()]);
        assert_eq!(hi.class.as_deref(), Some("real_time")); // has a deadline
        assert_eq!(hi.sched_class.as_deref(), Some("edf")); // EDF board
        assert_eq!(hi.deadline_us, Some(10_000));
        assert_eq!(hi.period_us, Some(20_000)); // 50 Hz
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

    use ros_launch_manifest_model::{Deploy, ExtraValue, Target};

    fn zephyr_deploy_with_edf(edf: bool) -> Deploy {
        let mut extra = std::collections::BTreeMap::new();
        extra.insert("edf".to_string(), ExtraValue::Bool(edf));
        Deploy {
            target: Some(Target::default()),
            extra,
            ..Default::default()
        }
    }

    #[test]
    fn deploy_knob_overrides_platform_edf_default() {
        // Platform default for zephyr is edf = true.
        assert!(sched_caps_for("zephyr").edf);
        // A deploy that turns edf OFF must be honored.
        let caps = sched_caps_from_deploy("zephyr", Some(&zephyr_deploy_with_edf(false)));
        assert!(
            !caps.edf,
            "deploy edf=false must override the platform default"
        );
        // A deploy with no edf key falls back to the platform default.
        let caps_default = sched_caps_from_deploy("zephyr", None);
        assert!(
            caps_default.edf,
            "no knob → platform default (true for zephyr)"
        );
    }

    #[test]
    fn deploy_edf_false_produces_accurate_degrade() {
        // The honesty property: edf=false → realize_rtos records a deadline Degrade.
        let input = input_two();
        let ranked = chain_aware_rank(&input);
        let caps = sched_caps_from_deploy("zephyr", Some(&zephyr_deploy_with_edf(false)));
        let plan = realize_rtos(&ranked, &input, &caps);
        let hi = plan.nodes.iter().find(|n| n.name == "/hi").unwrap();
        assert!(matches!(hi.deadline_real, DimRealization::Degrade { .. }));
        assert!(
            plan.degradations
                .iter()
                .any(|d| d.node == "/hi" && d.dim == "deadline")
        );
    }

    // ---- issue 0259: the derived blocking term `B_i` -----------------------

    fn node_with_execs(name: &str, execs: &[Option<f64>]) -> MapperNode {
        MapperNode {
            name: name.to_string(),
            scope: "/".to_string(),
            criticality: Some(Criticality::High),
            paths: execs
                .iter()
                .enumerate()
                .map(|(i, e)| timer_path(&format!("p{i}"), 50.0, Some(10.0), *e))
                .collect(),
            ..Default::default()
        }
    }

    fn realize_one(node: MapperNode) -> RealizedNode {
        let input = MapperInput {
            nodes: vec![node],
            legacy: None,
            chains: vec![],
        };
        let ranked = chain_aware_rank(&input);
        let plan = realize_rtos(&ranked, &input, &caps(false, false, false));
        plan.nodes.into_iter().next().expect("one node realized")
    }

    /// `B_i` is the longest OTHER callback — the second largest overall, since
    /// the longest cannot block itself.
    #[test]
    fn blocking_is_the_longest_sibling_not_the_longest_callback() {
        let n = realize_one(node_with_execs("/n", &[Some(3.0), Some(9.0), Some(5.0)]));
        assert_eq!(n.budget_us, Some(9_000), "budget is this node's own worst");
        assert_eq!(
            n.blocking_us,
            Some(5_000),
            "B_i must be the longest SIBLING (5ms), not the longest callback (9ms)"
        );
    }

    /// One callback cannot be blocked by a sibling it does not have. Absent,
    /// not zero — a feasibility check reading `None` as 0 is exactly the
    /// optimism issue 0259 is about.
    #[test]
    fn a_single_callback_has_no_blocking_term() {
        let n = realize_one(node_with_execs("/n", &[Some(4.0)]));
        assert_eq!(n.budget_us, Some(4_000));
        assert_eq!(n.blocking_us, None, "no sibling means no blocking term");
    }

    /// Undeclared WCETs yield no blocking term. The siblings exist; what is
    /// missing is any measurement of them, and inventing 0 would claim a
    /// callback waits for nothing.
    #[test]
    fn siblings_without_wcets_yield_no_blocking_term() {
        let n = realize_one(node_with_execs("/n", &[None, None, None]));
        assert_eq!(n.blocking_us, None);
        assert_eq!(n.budget_us, None);
    }

    /// A partially-measured node reports blocking only when TWO callbacks carry
    /// a WCET — one measured sibling is not a bound on the others.
    #[test]
    fn one_measured_callback_among_many_is_not_a_blocking_term() {
        let n = realize_one(node_with_execs("/n", &[Some(7.0), None, None]));
        assert_eq!(n.budget_us, Some(7_000));
        assert_eq!(
            n.blocking_us, None,
            "a lone measurement says nothing about how long the others run"
        );
    }

    /// The preemption THRESHOLD is deliberately still not derived: over a
    /// node's own callbacks the issue's ceiling equals the node's priority by
    /// construction, so deriving it would emit a tautology.
    #[test]
    fn the_preemption_threshold_is_not_derived_as_a_tautology() {
        let n = realize_one(node_with_execs("/n", &[Some(3.0), Some(9.0)]));
        assert_eq!(n.preempt_real, DimRealization::NotRequested);
        assert_eq!(n.preempt_threshold, None);
    }

    // ---- issue 0259: B_i's first consumer, the feasibility check -----------

    fn realize_one_plan(node: MapperNode) -> RtosPlan {
        let input = MapperInput {
            nodes: vec![node],
            legacy: None,
            chains: vec![],
        };
        let ranked = chain_aware_rank(&input);
        realize_rtos(&ranked, &input, &caps(false, false, false))
    }

    fn node_with(name: &str, deadline_ms: f64, execs: &[f64]) -> MapperNode {
        MapperNode {
            name: name.to_string(),
            scope: "/".to_string(),
            criticality: Some(Criticality::High),
            paths: execs
                .iter()
                .enumerate()
                .map(|(i, e)| timer_path(&format!("p{i}"), 50.0, Some(deadline_ms), Some(*e)))
                .collect(),
            ..Default::default()
        }
    }

    /// The blocking term CHANGES the verdict: 6ms of work fits a 10ms deadline,
    /// but 6ms plus a 5ms sibling does not. Without `B_i` this node would look
    /// schedulable — which is what issue 0259 is about.
    #[test]
    fn blocking_turns_a_fitting_node_into_an_infeasible_one() {
        let plan = realize_one_plan(node_with("/n", 10.0, &[6.0, 5.0]));
        let d = plan
            .degradations
            .iter()
            .find(|d| d.dim == "feasibility")
            .expect("C+B exceeds D, so the check must fire");
        assert_eq!(d.node, "/n");
        assert!(d.reason.contains("C=6000us"), "{}", d.reason);
        assert!(d.reason.contains("B=5000us"), "{}", d.reason);
        assert!(d.reason.contains("D=10000us"), "{}", d.reason);
    }

    /// The same work under a deadline that accommodates it stays silent.
    #[test]
    fn work_that_fits_within_the_deadline_is_not_reported() {
        let plan = realize_one_plan(node_with("/n", 20.0, &[6.0, 5.0]));
        assert!(
            !plan.degradations.iter().any(|d| d.dim == "feasibility"),
            "11ms of work fits a 20ms deadline: {:?}",
            plan.degradations
        );
    }

    /// A node whose own callback already exceeds the deadline is infeasible
    /// with no sibling at all — and the reason says the blocking term was
    /// unknown rather than implying it was measured as zero.
    #[test]
    fn a_single_overrunning_callback_reports_unknown_blocking() {
        let plan = realize_one_plan(node_with("/n", 4.0, &[9.0]));
        let d = plan
            .degradations
            .iter()
            .find(|d| d.dim == "feasibility")
            .expect("C alone exceeds D");
        assert!(
            d.reason.contains("unknown (counted as 0)"),
            "the report must not present an absent B as a measured 0: {}",
            d.reason
        );
    }

    /// No WCET means no verdict. Silence here is correct: the check reports on
    /// declarations, and an undeclared execution time is not a claim that the
    /// node fits.
    #[test]
    fn a_node_without_wcets_gets_no_feasibility_verdict() {
        let n = MapperNode {
            name: "/n".to_string(),
            scope: "/".to_string(),
            criticality: Some(Criticality::High),
            paths: vec![timer_path("p", 50.0, Some(1.0), None)],
            ..Default::default()
        };
        let plan = realize_one_plan(n);
        assert!(
            !plan.degradations.iter().any(|d| d.dim == "feasibility"),
            "an undeclared WCET must not produce a verdict either way"
        );
    }

    /// No deadline means no verdict — there is nothing to exceed.
    #[test]
    fn a_node_without_a_deadline_gets_no_feasibility_verdict() {
        let n = MapperNode {
            name: "/n".to_string(),
            scope: "/".to_string(),
            criticality: Some(Criticality::High),
            paths: vec![timer_path("p", 50.0, None, Some(900.0))],
            ..Default::default()
        };
        let plan = realize_one_plan(n);
        assert!(!plan.degradations.iter().any(|d| d.dim == "feasibility"));
    }

    // ---- issue 0259: system utilisation ------------------------------------

    fn caps_cores(n: Option<u16>) -> SchedCaps {
        SchedCaps {
            n_cores: n,
            ..caps(false, false, false)
        }
    }

    /// Two nodes each needing 60% of a processor cannot share one.
    #[test]
    fn oversubscribed_uniprocessor_is_reported_against_its_declared_cores() {
        let input = MapperInput {
            nodes: vec![
                node_with_rate("/a", 100.0, 6.0),
                node_with_rate("/b", 100.0, 6.0),
            ],
            legacy: None,
            chains: vec![],
        };
        let ranked = chain_aware_rank(&input);
        let plan = realize_rtos(&ranked, &input, &caps_cores(Some(1)));
        let u = plan
            .degradations
            .iter()
            .find(|d| d.dim == "utilization")
            .expect("1.20 processors demanded against 1 declared");
        assert_eq!(u.node, "<system>");
        assert!(u.reason.contains("1.20 processors"), "{}", u.reason);
        assert!(u.reason.contains("declares 1"), "{}", u.reason);
    }

    /// The SAME taskset on two declared cores fits, and is not reported. This
    /// is what the core count buys: the verdict depends on the hardware, not on
    /// a capability flag that cannot express a quantity.
    #[test]
    fn the_same_taskset_fits_when_two_cores_are_declared() {
        let input = MapperInput {
            nodes: vec![
                node_with_rate("/a", 100.0, 6.0),
                node_with_rate("/b", 100.0, 6.0),
            ],
            legacy: None,
            chains: vec![],
        };
        let ranked = chain_aware_rank(&input);
        let plan = realize_rtos(&ranked, &input, &caps_cores(Some(2)));
        assert!(!plan.degradations.iter().any(|d| d.dim == "utilization"));
    }

    /// No declared core count means no verdict. A bake cannot infer the
    /// hardware, and guessing either way fabricates it.
    #[test]
    fn an_undeclared_core_count_yields_no_utilization_verdict() {
        let input = MapperInput {
            nodes: vec![
                node_with_rate("/a", 100.0, 6.0),
                node_with_rate("/b", 100.0, 6.0),
            ],
            legacy: None,
            chains: vec![],
        };
        let ranked = chain_aware_rank(&input);
        let plan = realize_rtos(&ranked, &input, &caps_cores(None));
        assert!(
            !plan.degradations.iter().any(|d| d.dim == "utilization"),
            "unknown cores must be silent, not assumed to be 1"
        );
    }

    /// Unmeasured nodes are NAMED, so a total cannot be read as "the system
    /// fits" when part of the taskset contributed nothing.
    #[test]
    fn unmeasured_nodes_are_named_in_the_utilization_verdict() {
        let mut unmeasured = node_with_rate("/silent", 100.0, 0.0);
        unmeasured.paths[0].exec_ms = None;
        let input = MapperInput {
            nodes: vec![
                node_with_rate("/a", 100.0, 6.0),
                node_with_rate("/b", 100.0, 6.0),
                unmeasured,
            ],
            legacy: None,
            chains: vec![],
        };
        let ranked = chain_aware_rank(&input);
        let plan = realize_rtos(&ranked, &input, &caps_cores(Some(1)));
        let u = plan
            .degradations
            .iter()
            .find(|d| d.dim == "utilization")
            .expect("still oversubscribed");
        assert!(u.reason.contains("/silent"), "{}", u.reason);
        assert!(
            u.reason.contains("the real total is higher"),
            "{}",
            u.reason
        );
    }

    /// A zero core count is a typo, not a claim: ignored rather than honoured.
    #[test]
    fn a_zero_core_count_is_ignored_rather_than_divided_by() {
        use ros_launch_manifest_model::{Deploy, ExtraValue};
        let mut d = Deploy::default();
        d.extra.insert("cores".into(), ExtraValue::Int(0));
        assert_eq!(sched_caps_from_deploy("zephyr", Some(&d)).n_cores, None);

        d.extra.insert("cores".into(), ExtraValue::Int(4));
        assert_eq!(sched_caps_from_deploy("zephyr", Some(&d)).n_cores, Some(4));
    }

    fn node_with_rate(name: &str, rate_hz: f64, exec_ms: f64) -> MapperNode {
        MapperNode {
            name: name.to_string(),
            scope: "/".to_string(),
            criticality: Some(Criticality::High),
            paths: vec![timer_path("p", rate_hz, None, Some(exec_ms))],
            ..Default::default()
        }
    }
}
