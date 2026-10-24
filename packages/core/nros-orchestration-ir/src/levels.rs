//! phase-509 P5 (RFC-0108 D3/D4) -- preemption levels, derived from the
//! contract.
//!
//! A LEVEL is a contiguous band of ranks that one worker dispatches without
//! preempting itself; levels preempt each other through the kernel. Until the
//! scheduler/worker split (P6) and the level runners (P9) land, a derived
//! level is realised as one derived tier: one thread at the band's most
//! urgent priority, whose members dispatch in rank order inside it (P2).
//!
//! # The rules (RFC-0108 D4)
//!
//! 1. **Default:** one level per distinct rank. Equal ranks share a level.
//! 2. **Merge with execution times.** When every callback of the free nodes
//!    is analysable and carries C (the task record's `exec_us`, or a
//!    server's budget), a dynamic program over the band boundaries finds the
//!    fewest contiguous bands in which every callback still meets its
//!    deadline under the preemption-threshold response time
//!    ([`response_time`](crate::levels::response_time)). Ties go to the largest minimum normalised slack
//!    `(D - R) / D`.
//! 3. **Merge without execution times,** only to meet the cap: merge the
//!    least urgent adjacent pair, skipping a pair whose more urgent band
//!    holds a deadline or a supervised route. Only when every remaining pair
//!    touches such a band is one merged anyway, with a degradation naming the
//!    blocking it adds.
//! 4. **The cap** is a count, `max_levels`: the image's
//!    `CONFIG_NROS_EXECUTOR_MAX_LEVELS` when its `.config` is in hand
//!    ([`crate::priority_plan::PriorityPlan::max_levels`]), else the
//!    platform's default ([`default_max_levels`](crate::levels::default_max_levels), RFC-0108 D6).
//! 5. **Report:** one `level-budget` degradation per member that loses its
//!    rank, naming the cap, the rank it would have had and the band it
//!    shares.
//! 6. **Authored tiers are pins:** each is one band at its own priority, and
//!    the derivation bands only the nodes no tier binds. A free band never
//!    spans a pin's priority.
//! 7. (The cost report is P9's.)
//! 8. **Equal kernel priorities merge** on a time-slicing kernel
//!    ([`time_slices_equal_priorities`](crate::levels::time_slices_equal_priorities)): two levels at one priority there
//!    buy no preemption, only memory, and slicing would cut a callback.
//!
//! An unanalysable callback (RFC-0108 D1) is placed at the least urgent rank
//! of its level ([`Level::best_effort`](crate::levels::Level::best_effort)), so it never orders ahead of an
//! analysed member of its band.
//!
//! # Where the response-time analysis lives (RFC-0108, deferred to P5)
//!
//! Here, in `nros-orchestration-ir`, beside the derivation that consumes it.
//! The analysis needs what only this side has: the board's priority plan,
//! the authored tiers that pin bands, the task records with their C, and the
//! cap. rlm's ranker orders the work and stays platform-free; moving the
//! analysis there would make rlm read board facts, or make this crate hand
//! its bands back across the pin boundary.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{
    priority_plan::PriorityPlan,
    rtos_realizer::Degradation,
    task_record::{ReleaseKind, TaskRecord},
};

/// The platform's default level cap (RFC-0108 D6), when the image states
/// none. `plan` gives the pool width, which is ThreadX's default (its native
/// preemption threshold makes a level cheap).
pub fn default_max_levels(target_rtos: &str, plan: &PriorityPlan) -> u16 {
    match target_rtos {
        "zephyr" | "posix" | "native" | "nuttx" => 8,
        "threadx" => u16::try_from(plan.app.width()).unwrap_or(u16::MAX).max(1),
        "freertos" => 4,
        // No RTOS family: one thread, nothing to preempt with.
        _ => 1,
    }
}

/// Does this kernel time-slice threads of equal priority (RFC-0108 D4 rule
/// 8, D6)? FreeRTOS does by default (`configUSE_TIME_SLICING`). Linux and
/// NuttX levels run `SCHED_FIFO`, never `SCHED_RR`; Zephyr's tiers run with
/// `CONFIG_TIMESLICE_SIZE` 0; ThreadX's tier threads take no time slice.
pub fn time_slices_equal_priorities(target_rtos: &str) -> bool {
    target_rtos == "freertos"
}

/// A ranked node the derivation may band.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelNode {
    /// The node FQN.
    pub node: String,
    /// Its allocated kernel priority, in the board's direction.
    pub priority: i64,
    /// Why a capped merge must not add blocking to it: `"deadline"` (the
    /// contract states one) or `"supervised"` (a supervised route).
    pub protected: Option<&'static str>,
}

/// An authored tier: a pinned band (RFC-0108 D4 rule 6).
#[derive(Clone, Debug, PartialEq)]
pub struct PinnedBand {
    pub tier: String,
    /// The tier's kernel priority: authored, or allocated from its most
    /// urgent member (issue 1426 rule 2).
    pub priority: i64,
    /// Its members, each at its own allocated priority (its rank).
    pub members: Vec<LevelNode>,
}

/// How the bands were chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LevelMethod {
    /// Rule 1: one level per distinct rank, nothing merged.
    PerRank,
    /// Rule 2: the fewest bands that keep every deadline.
    ResponseTime,
    /// Rule 3: merged from the least urgent end to meet the cap.
    Capped,
    /// Rule 2, then rule 3 because the fewest feasible bands exceed the cap.
    ResponseTimeCapped,
}

impl LevelMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            LevelMethod::PerRank => "per-rank",
            LevelMethod::ResponseTime => "response-time",
            LevelMethod::Capped => "capped",
            LevelMethod::ResponseTimeCapped => "response-time-capped",
        }
    }
}

/// One member of a level.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LevelMember {
    /// The node FQN.
    pub node: String,
    /// Its normalised rank (0 = most urgent), the dispatch key inside the
    /// level (RFC-0108 D2).
    pub rank: u8,
    /// Its allocated kernel priority.
    pub priority: i64,
    /// The authored tier that pins it, `None` for a derived member.
    pub tier: Option<String>,
}

/// One preemption level (RFC-0108 D3).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Level {
    /// 0 is the most urgent level.
    pub index: usize,
    /// The level's kernel priority: its most urgent member's, or the pin's.
    pub priority: i64,
    /// Every member's threshold: the most urgent rank of the band.
    pub threshold: u8,
    /// The members, most urgent first.
    pub members: Vec<LevelMember>,
    /// The authored tiers this level is (empty for a derived level).
    pub tiers: Vec<String>,
    /// Unanalysable callbacks of its members, each dispatched at the band's
    /// least urgent rank (RFC-0108 D1): `(callback, rank)`.
    pub best_effort: Vec<(String, u8)>,
}

impl Level {
    /// The free (unpinned) members.
    pub fn derived_members(&self) -> impl Iterator<Item = &LevelMember> {
        self.members.iter().filter(|m| m.tier.is_none())
    }
}

/// The derived levels of one core (RFC-0108 D4).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LevelPlan {
    /// The core these levels run on: the realizer's placement of their
    /// members (issue 0259), `None` for the nodes it left unpinned.
    pub core: Option<u32>,
    /// The cap the levels were derived against.
    pub max_levels: u16,
    /// Where the cap came from: `"image"` (its `.config`) or `"platform"`.
    pub cap_source: &'static str,
    pub method: LevelMethod,
    pub levels: Vec<Level>,
    /// The `level-budget`, `level` and `feasibility` reports.
    #[serde(skip)]
    pub degradations: Vec<Degradation>,
}

/// What [`derive_levels`] reads.
#[derive(Clone, Debug)]
pub struct LevelInput<'a> {
    /// The core every node in `free` is placed on (`None`: unpinned). The
    /// caller partitions first, so one call never spans two cores' scheduling
    /// domains (RFC-0108 D3, issue 0259).
    pub core: Option<u32>,
    /// Work this call does not see can preempt on this core: unpinned
    /// nodes beside placed ones. Their interference is not in `tasks`, so the
    /// analysed merge (rule 2) could not bound it and stays off, as it does
    /// for a pin.
    pub floating_interference: bool,
    /// The nodes no authored tier binds.
    pub free: &'a [LevelNode],
    /// The authored tiers.
    pub pinned: &'a [PinnedBand],
    /// Every callback's task record (RFC-0108 D1).
    pub tasks: &'a [TaskRecord],
    pub max_levels: u16,
    pub cap_source: &'static str,
    /// `true` when a smaller number is more urgent (Zephyr, ThreadX).
    pub low_number_is_high: bool,
    /// [`time_slices_equal_priorities`].
    pub time_slicing: bool,
    /// The priority of every node the contract ranked, groupless ones too,
    /// so a member's rank is the one codegen seeds ([`node_ranks`]).
    pub ranked_priorities: &'a [i64],
}

/// Each ranked node's normalised rank: the distinct priorities in urgency
/// order, 0 the most urgent, equal priorities one rank (RFC-0108 D2). The
/// one normalisation the C/C++ packs and the Rust entry seed with
/// (`bind_node_rank`), and the levels' ranks.
pub fn node_ranks(
    realized: &[crate::rtos_realizer::RealizedNode],
    smaller_is_urgent: bool,
) -> Vec<(String, u8)> {
    let mut distinct: Vec<i64> = realized.iter().map(|n| n.priority).collect();
    distinct.sort_unstable();
    distinct.dedup();
    if !smaller_is_urgent {
        distinct.reverse();
    }
    realized
        .iter()
        .map(|n| {
            let rank = distinct.iter().position(|p| *p == n.priority).unwrap_or(0);
            (n.name.clone(), u8::try_from(rank).unwrap_or(u8::MAX))
        })
        .collect()
}

/// One analysable callback, for [`response_time`].
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub callback: String,
    /// Its band position: the index of its rank among the analysed ranks,
    /// 0 the most urgent.
    pub rank: usize,
    /// C, T and D in microseconds.
    pub c: u64,
    pub t: u64,
    pub d: u64,
}

/// The job's C, T and D, or `None` when the record cannot be analysed.
fn job_of(t: &TaskRecord, rank: usize) -> Option<Job> {
    let (c, period) = match t.release {
        ReleaseKind::Periodic | ReleaseKind::Sporadic => (t.exec_us?, t.period_us?),
        // A served callback is the periodic task (Q, P) to the analysis.
        ReleaseKind::Served => (t.budget_us?, t.budget_period_us?),
        ReleaseKind::Unanalysable => return None,
    };
    if period == 0 {
        return None;
    }
    Some(Job {
        callback: t.callback.clone(),
        rank,
        c,
        t: period,
        d: t.deadline_us?,
    })
}

/// Most busy-period jobs examined before the analysis gives up (and calls
/// the band infeasible: it could not prove the deadline).
const MAX_JOBS: u64 = 100_000;

/// The worst-case response time of `jobs[i]` when its band covers the ranks
/// `top..=bottom`, under fixed-priority preemption-threshold scheduling
/// (Wang and Saksena 1999, with every job of the level-i busy period
/// examined; RFC-0108 D3). `None` when it cannot be bounded within the
/// job's deadline (the band misses it).
///
/// - Blocking `B` is the largest C of a less urgent member of the band (a
///   started callback runs to completion).
/// - Before the job starts, every other job of equal or more urgent rank
///   interferes (equal ranks are FIFO, so one released first goes first).
/// - Once it runs, only bands more urgent than its own preempt it.
pub fn response_time(jobs: &[Job], i: usize, top: usize, bottom: usize) -> Option<u64> {
    let me = &jobs[i];
    let blocking = jobs
        .iter()
        .filter(|j| j.rank > me.rank && j.rank <= bottom)
        .map(|j| j.c)
        .max()
        .unwrap_or(0);
    let hp: Vec<&Job> = jobs
        .iter()
        .enumerate()
        .filter(|(k, j)| *k != i && j.rank <= me.rank)
        .map(|(_, j)| j)
        .collect();
    let preempt: Vec<&Job> = jobs.iter().filter(|j| j.rank < top).collect();

    // The level-i busy period.
    let util: f64 =
        hp.iter().map(|j| j.c as f64 / j.t as f64).sum::<f64>() + me.c as f64 / me.t as f64;
    if util > 1.0 {
        return None;
    }
    let mut l = blocking + me.c + hp.iter().map(|j| j.c).sum::<u64>();
    loop {
        let next = blocking
            + me.c * l.div_ceil(me.t)
            + hp.iter().map(|j| j.c * l.div_ceil(j.t)).sum::<u64>();
        if next == l {
            break;
        }
        if next > me.t.saturating_mul(MAX_JOBS) {
            return None;
        }
        l = next;
    }
    let jobs_in_busy = l.div_ceil(me.t).max(1);

    let mut worst = 0u64;
    for q in 0..jobs_in_busy {
        // Start time.
        let mut s = blocking + q * me.c + hp.iter().map(|j| j.c).sum::<u64>();
        loop {
            let next = blocking + q * me.c + hp.iter().map(|j| (s / j.t + 1) * j.c).sum::<u64>();
            if next == s {
                break;
            }
            if next + me.c > q * me.t + me.d {
                return None;
            }
            s = next;
        }
        // Finish time.
        let mut f = s + me.c;
        loop {
            let next = s
                + me.c
                + preempt
                    .iter()
                    .map(|j| (f.div_ceil(j.t).saturating_sub(s / j.t + 1)) * j.c)
                    .sum::<u64>();
            if next == f {
                break;
            }
            if next > q * me.t + me.d {
                return None;
            }
            f = next;
        }
        let r = f - q * me.t;
        if r > me.d {
            return None;
        }
        worst = worst.max(r);
    }
    Some(worst)
}

/// The least normalised slack `(D - R) / D` of the band `top..=bottom`'s
/// jobs, or `None` when one of them misses its deadline.
fn band_slack(jobs: &[Job], top: usize, bottom: usize) -> Option<f64> {
    let mut least = f64::INFINITY;
    for (i, j) in jobs.iter().enumerate() {
        if j.rank < top || j.rank > bottom {
            continue;
        }
        let r = response_time(jobs, i, top, bottom)?;
        let slack = (j.d - r) as f64 / j.d.max(1) as f64;
        least = least.min(slack);
    }
    Some(least)
}

/// Rule 2: the fewest contiguous bands over `n` ranks in which every job
/// meets its deadline, ties to the largest least slack. Each band is
/// `(top, bottom)`. `None` when no banding keeps every deadline.
///
/// A job's response time depends only on its own band (blocking comes from
/// below it in the band, preemption from the bands above), so a band's
/// feasibility is independent of the others and the program is O(n^2) band
/// analyses.
pub fn fewest_feasible_bands(jobs: &[Job], n: usize) -> Option<Vec<(usize, usize)>> {
    // best[e]: the best banding of ranks 0..e, as (bands, least slack, prev).
    let mut best: Vec<Option<(usize, f64, usize)>> = vec![None; n + 1];
    best[0] = Some((0, f64::INFINITY, 0));
    for e in 1..=n {
        for s in 0..e {
            let Some((count, slack, _)) = best[s] else {
                continue;
            };
            let Some(band) = band_slack(jobs, s, e - 1) else {
                continue;
            };
            let cand = (count + 1, slack.min(band), s);
            let better = match best[e] {
                None => true,
                Some((c, sl, _)) => cand.0 < c || (cand.0 == c && cand.1 > sl),
            };
            if better {
                best[e] = Some(cand);
            }
        }
    }
    best[n]?;
    let mut bands = Vec::new();
    let mut e = n;
    while e > 0 {
        let (_, _, s) = best[e]?;
        bands.push((s, e - 1));
        e = s;
    }
    bands.reverse();
    Some(bands)
}

/// A free band during the derivation: a run of free ranks inside one
/// segment (free ranks between two pins).
#[derive(Clone, Debug)]
struct FreeBand {
    segment: usize,
    /// Indices into the free ranks, `top..=bottom`.
    top: usize,
    bottom: usize,
}

/// Derive one core's levels (RFC-0108 D4).
pub fn derive_levels(input: &LevelInput<'_>) -> LevelPlan {
    let urgent_first = |a: &i64, b: &i64| {
        if input.low_number_is_high {
            a.cmp(b)
        } else {
            b.cmp(a)
        }
    };
    // The normalised rank of every priority a ranked node holds.
    let mut distinct: Vec<i64> = input
        .free
        .iter()
        .chain(input.pinned.iter().flat_map(|p| p.members.iter()))
        .map(|n| n.priority)
        .chain(input.ranked_priorities.iter().copied())
        .collect();
    distinct.sort_by(urgent_first);
    distinct.dedup();
    let rank_of = |p: i64| -> u8 {
        let r = distinct.iter().position(|d| *d == p).unwrap_or(0);
        u8::try_from(r).unwrap_or(u8::MAX)
    };

    // The free ranks, most urgent first, each with its nodes.
    let mut free_prios: Vec<i64> = input.free.iter().map(|n| n.priority).collect();
    free_prios.sort_by(urgent_first);
    free_prios.dedup();
    let nodes_at =
        |p: i64| -> Vec<&LevelNode> { input.free.iter().filter(|n| n.priority == p).collect() };
    // Segments: a pin's priority strictly between two free ranks splits
    // them, so no free band spans a pinned band (contiguity).
    let mut segment_of: Vec<usize> = Vec::with_capacity(free_prios.len());
    let mut seg = 0usize;
    for (k, p) in free_prios.iter().enumerate() {
        if k > 0 {
            let prev = free_prios[k - 1];
            let splits = input.pinned.iter().any(|pin| {
                urgent_first(&prev, &pin.priority).is_lt() && urgent_first(&pin.priority, p).is_lt()
            });
            if splits {
                seg += 1;
            }
        }
        segment_of.push(seg);
    }
    let segments = if free_prios.is_empty() { 0 } else { seg + 1 };

    let mut degradations: Vec<Degradation> = Vec::new();
    let cap = usize::from(input.max_levels.max(1));
    let free_budget = cap.saturating_sub(input.pinned.len()).max(segments);
    if input.pinned.len() + segments > cap {
        degradations.push(Degradation {
            node: format!("max_levels = {}", input.max_levels),
            dim: "level-budget",
            reason: format!(
                "the cap of {} level(s) cannot be met: {} authored tier(s) are pins \
                 (RFC-0108 D4 rule 6) and the derived nodes between them need {} \
                 level(s) at least, so this core runs {} levels",
                input.max_levels,
                input.pinned.len(),
                segments,
                input.pinned.len() + segments
            ),
        });
    }

    // Rule 1: one band per free rank.
    let per_rank: Vec<FreeBand> = (0..free_prios.len())
        .map(|k| FreeBand {
            segment: segment_of[k],
            top: k,
            bottom: k,
        })
        .collect();

    // Rule 2: the analysed merge, when every free callback has C.
    let free_nodes: BTreeSet<&str> = input.free.iter().map(|n| n.node.as_str()).collect();
    let free_tasks: Vec<&TaskRecord> = input
        .tasks
        .iter()
        .filter(|t| free_nodes.contains(t.node.as_str()))
        .collect();
    let rank_index = |node: &str| -> Option<usize> {
        let p = input.free.iter().find(|n| n.node == node)?.priority;
        free_prios.iter().position(|q| *q == p)
    };
    let jobs: Option<Vec<Job>> =
        if input.pinned.is_empty() && !input.floating_interference && !free_tasks.is_empty() {
            free_tasks
                .iter()
                .map(|t| job_of(t, rank_index(&t.node)?))
                .collect()
        } else {
            None
        };
    let mut method = LevelMethod::PerRank;
    let mut bands = per_rank.clone();
    let mut analysed = false;
    if let Some(jobs) = jobs.as_ref() {
        match fewest_feasible_bands(jobs, free_prios.len()) {
            Some(found) => {
                analysed = true;
                if found.len() < per_rank.len() {
                    method = LevelMethod::ResponseTime;
                }
                bands = found
                    .into_iter()
                    .map(|(top, bottom)| FreeBand {
                        segment: 0,
                        top,
                        bottom,
                    })
                    .collect();
            }
            None => {
                for (i, j) in jobs.iter().enumerate() {
                    if response_time(jobs, i, j.rank, j.rank).is_none() {
                        degradations.push(Degradation {
                            node: j.callback.clone(),
                            dim: "feasibility",
                            reason: format!(
                                "no banding of this core's ranks keeps every deadline \
                                 (RFC-0108 D4 rule 2): with a level per rank this \
                                 callback (C={}us, T={}us) still misses D={}us, so the \
                                 levels stay one per rank",
                                j.c, j.t, j.d
                            ),
                        });
                    }
                }
            }
        }
    }

    // Rule 3: merge from the least urgent end until the cap fits.
    let protected_of = |b: &FreeBand| -> Option<(&'static str, &str)> {
        free_prios[b.top..=b.bottom].iter().find_map(|p| {
            nodes_at(*p)
                .into_iter()
                .find_map(|n| n.protected.map(|why| (why, n.node.as_str())))
        })
    };
    while bands.len() > free_budget {
        let pairs: Vec<usize> = (0..bands.len().saturating_sub(1))
            .rev()
            .filter(|&i| bands[i].segment == bands[i + 1].segment)
            .collect();
        let Some(&first) = pairs.first() else {
            break;
        };
        let pick = pairs
            .iter()
            .copied()
            .find(|&i| protected_of(&bands[i]).is_none());
        let i = match pick {
            Some(i) => i,
            None => {
                // Every remaining pair touches a protected band: merge the
                // least urgent anyway, and name the blocking it adds.
                let (why, holder) = protected_of(&bands[first]).unwrap_or(("deadline", "?"));
                let added: Vec<String> = free_prios[bands[first + 1].top..=bands[first + 1].bottom]
                    .iter()
                    .flat_map(|p| nodes_at(*p))
                    .map(|n| format!("`{}`", n.node))
                    .collect();
                degradations.push(Degradation {
                    node: holder.to_string(),
                    dim: "level-budget",
                    reason: format!(
                        "max_levels = {} forces a merge into the band holding this node's \
                         {why}: every remaining pair touches a deadline or a supervised \
                         route (RFC-0108 D4 rule 3). A started callback of {} now blocks \
                         it for up to that callback's execution time.",
                        input.max_levels,
                        added.join(", ")
                    ),
                });
                first
            }
        };
        bands[i].bottom = bands[i + 1].bottom;
        bands.remove(i + 1);
        method = if analysed {
            LevelMethod::ResponseTimeCapped
        } else {
            LevelMethod::Capped
        };
    }

    // The levels: the free bands and the pins, most urgent first.
    let mut levels: Vec<Level> = Vec::new();
    for b in &bands {
        let members: Vec<LevelMember> = free_prios[b.top..=b.bottom]
            .iter()
            .flat_map(|p| nodes_at(*p))
            .map(|n| LevelMember {
                node: n.node.clone(),
                rank: rank_of(n.priority),
                priority: n.priority,
                tier: None,
            })
            .collect();
        levels.push(Level {
            index: 0,
            priority: free_prios[b.top],
            threshold: rank_of(free_prios[b.top]),
            members,
            tiers: Vec::new(),
            best_effort: Vec::new(),
        });
    }
    for pin in input.pinned {
        let mut members: Vec<LevelMember> = pin
            .members
            .iter()
            .map(|n| LevelMember {
                node: n.node.clone(),
                rank: rank_of(n.priority),
                priority: n.priority,
                tier: Some(pin.tier.clone()),
            })
            .collect();
        members.sort_by_key(|m| m.rank);
        levels.push(Level {
            index: 0,
            priority: pin.priority,
            threshold: members.first().map_or(0, |m| m.rank),
            members,
            tiers: vec![pin.tier.clone()],
            best_effort: Vec::new(),
        });
    }
    levels.sort_by(|a, b| {
        urgent_first(&a.priority, &b.priority).then(b.tiers.len().cmp(&a.tiers.len()))
    });

    // Rule 8: one level per kernel priority on a time-slicing kernel.
    if input.time_slicing {
        let mut merged: Vec<Level> = Vec::new();
        for l in levels {
            match merged.last_mut() {
                Some(prev) if prev.priority == l.priority => {
                    let names: Vec<String> =
                        l.members.iter().map(|m| format!("`{}`", m.node)).collect();
                    degradations.push(Degradation {
                        node: names.join(", "),
                        dim: "level",
                        reason: format!(
                            "two levels would run at kernel priority {} on a kernel \
                             that time-slices equal priorities, which buys no \
                             preemption between them and would cut a callback mid-run \
                             (RFC-0108 D4 rule 8): they are one level",
                            l.priority
                        ),
                    });
                    prev.members.extend(l.members);
                    prev.members.sort_by_key(|m| m.rank);
                    prev.threshold = prev.members.first().map_or(prev.threshold, |m| m.rank);
                    for t in l.tiers {
                        if !prev.tiers.contains(&t) {
                            prev.tiers.push(t);
                        }
                    }
                }
                _ => merged.push(l),
            }
        }
        levels = merged;
    }

    // Rule 5: every free member that loses its rank. (A pinned member's
    // loss is the authored tier's, reported by tier derivation rule 2.)
    for (k, l) in levels.iter().enumerate() {
        let lead = l
            .members
            .first()
            .map(|m| m.node.clone())
            .unwrap_or_default();
        for m in l.derived_members() {
            if m.rank == l.threshold {
                continue;
            }
            let others: Vec<String> = l
                .members
                .iter()
                .filter(|o| o.node != m.node)
                .map(|o| format!("`{}`", o.node))
                .collect();
            let why = match method {
                LevelMethod::ResponseTime => format!(
                    "the response-time analysis merged its band with `{lead}`'s because \
                     every deadline still holds with fewer levels"
                ),
                _ => format!(
                    "the image caps this core at max_levels = {} ({}), so its band was \
                     merged",
                    input.max_levels, input.cap_source
                ),
            };
            degradations.push(Degradation {
                node: m.node.clone(),
                dim: "level-budget",
                reason: format!(
                    "{why}: it would have had rank {} at priority {} and its own level; it \
                     now shares level {} (threshold rank {}, priority {}) with {}, so a \
                     started callback of a less urgent member of the band blocks it and it \
                     no longer preempts them (RFC-0108 D4 rule 5)",
                    m.rank,
                    m.priority,
                    k,
                    l.threshold,
                    l.priority,
                    others.join(", ")
                ),
            });
        }
    }
    for (k, l) in levels.iter_mut().enumerate() {
        l.index = k;
    }

    // Unanalysable callbacks go to the least urgent rank of their level.
    for l in &mut levels {
        let floor = l.members.iter().map(|m| m.rank).max().unwrap_or(0);
        for t in input.tasks {
            if t.release == ReleaseKind::Unanalysable && l.members.iter().any(|m| m.node == t.node)
            {
                l.best_effort.push((t.callback.clone(), floor));
            }
        }
    }

    LevelPlan {
        core: input.core,
        max_levels: input.max_levels,
        cap_source: input.cap_source,
        method,
        levels,
        degradations,
    }
}

/// `node -> level index`, for the record and the tests.
pub fn level_of(plan: &LevelPlan) -> BTreeMap<String, usize> {
    plan.levels
        .iter()
        .flat_map(|l| l.members.iter().map(move |m| (m.node.clone(), l.index)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_record::{CallbackKind, ReleaseSource};

    fn node(name: &str, priority: i64) -> LevelNode {
        LevelNode {
            node: name.to_string(),
            priority,
            protected: None,
        }
    }

    fn input<'a>(
        free: &'a [LevelNode],
        pinned: &'a [PinnedBand],
        tasks: &'a [TaskRecord],
        cap: u16,
    ) -> LevelInput<'a> {
        LevelInput {
            core: None,
            floating_interference: false,
            free,
            pinned,
            tasks,
            max_levels: cap,
            cap_source: "image",
            low_number_is_high: true,
            time_slicing: false,
            ranked_priorities: &[],
        }
    }

    fn members(plan: &LevelPlan) -> Vec<Vec<&str>> {
        plan.levels
            .iter()
            .map(|l| l.members.iter().map(|m| m.node.as_str()).collect())
            .collect()
    }

    fn timer(node: &str, c: Option<u64>, t: u64) -> TaskRecord {
        TaskRecord {
            callback: format!("{node}/tick"),
            node: node.to_string(),
            kind: CallbackKind::Timer,
            release: ReleaseKind::Periodic,
            source: ReleaseSource::TimerRate,
            period_us: Some(t),
            deadline_us: Some(t),
            deadline_stated: false,
            exec_us: c,
            budget_us: None,
            budget_period_us: None,
        }
    }

    fn level_budget(plan: &LevelPlan) -> Vec<&str> {
        plan.degradations
            .iter()
            .filter(|d| d.dim == "level-budget")
            .map(|d| d.node.as_str())
            .collect()
    }

    /// Rule 1: equal ranks share a level, so two equal-rank nodes plus one
    /// other give 2 levels (rule 4 of tier derivation gave 3 tiers).
    #[test]
    fn two_equal_rank_nodes_and_one_other_give_two_levels() {
        let free = [node("/a", 5), node("/b", 5), node("/c", 6)];
        let plan = derive_levels(&input(&free, &[], &[], 8));
        assert_eq!(members(&plan), vec![vec!["/a", "/b"], vec!["/c"]]);
        assert_eq!(plan.method, LevelMethod::PerRank);
        assert!(level_budget(&plan).is_empty(), "{:?}", plan.degradations);
        assert_eq!(plan.levels[0].threshold, 0);
        assert_eq!(plan.levels[1].threshold, 1);
    }

    /// Rule 3: cap 2 over 3 ranks merges the least urgent pair, and the
    /// member that loses its rank is reported by name.
    #[test]
    fn cap_two_over_three_ranks_merges_the_least_urgent_pair() {
        let free = [node("/h", 5), node("/e", 7), node("/c", 8)];
        let plan = derive_levels(&input(&free, &[], &[], 2));
        assert_eq!(members(&plan), vec![vec!["/h"], vec!["/e", "/c"]]);
        assert_eq!(plan.method, LevelMethod::Capped);
        assert_eq!(level_budget(&plan), vec!["/c"]);
        let line = &plan.degradations[0].reason;
        assert!(
            line.contains("max_levels = 2") && line.contains("rank 2"),
            "{line}"
        );
        assert!(line.contains("`/e`"), "names the band it shares: {line}");
    }

    /// Rule 3 skips a band that holds a deadline or a supervised route: the
    /// island's 2-level fallback keeps `mrm_handler` alone even when the
    /// least urgent pair is protected first.
    #[test]
    fn a_protected_band_is_skipped_by_the_capped_merge() {
        let mut free = [node("/h", 5), node("/e", 7), node("/c", 8)];
        free[1].protected = Some("deadline");
        let plan = derive_levels(&input(&free, &[], &[], 2));
        // (e, c) would add c's blocking to e's deadline, so the least urgent
        // pair whose more urgent band is unprotected is (h, e).
        assert_eq!(members(&plan), vec![vec!["/h", "/e"], vec!["/c"]]);
        assert_eq!(level_budget(&plan), vec!["/e"]);

        // The island: the handler is supervised, the operators are not.
        let mut island = [
            node("/mrm_handler", 5),
            node("/mrm_emergency_stop_operator", 7),
            node("/mrm_comfortable_stop_operator", 8),
        ];
        island[0].protected = Some("supervised");
        let plan = derive_levels(&input(&island, &[], &[], 2));
        assert_eq!(
            members(&plan),
            vec![
                vec!["/mrm_handler"],
                vec![
                    "/mrm_emergency_stop_operator",
                    "/mrm_comfortable_stop_operator"
                ]
            ]
        );
        let plan3 = derive_levels(&input(&island, &[], &[], 8));
        assert_eq!(plan3.levels.len(), 3, "one level per rank by default");
    }

    /// Only when every remaining pair touches a protected band is one merged,
    /// and that merge is reported with the blocking it adds.
    #[test]
    fn a_forced_merge_names_the_blocking_it_adds() {
        let mut free = [node("/h", 5), node("/e", 7)];
        free[0].protected = Some("supervised");
        let plan = derive_levels(&input(&free, &[], &[], 1));
        assert_eq!(plan.levels.len(), 1);
        let forced = plan
            .degradations
            .iter()
            .find(|d| d.node == "/h")
            .expect("the protected node is named");
        assert!(forced.reason.contains("`/e`"), "{}", forced.reason);
    }

    /// Cap 1 reports every member that loses its rank.
    #[test]
    fn cap_one_reports_every_member() {
        let free = [node("/a", 5), node("/b", 6), node("/c", 7), node("/d", 7)];
        let plan = derive_levels(&input(&free, &[], &[], 1));
        assert_eq!(plan.levels.len(), 1);
        assert_eq!(level_budget(&plan), vec!["/b", "/c", "/d"]);
    }

    /// The fixture where the naive merge misses a deadline: A (C 1, T 4),
    /// B (C 2, T 8), C (C 6, T 40), implicit deadlines. Merging the least
    /// urgent pair {B, C} lets C block B for 6 us, and B misses 8 us. The
    /// analysed merge finds {A, B} | {C}, which keeps every deadline, in as
    /// few levels.
    #[test]
    fn with_c_stated_the_merge_keeps_every_deadline_where_the_naive_merge_misses() {
        let free = [node("/a", 5), node("/b", 6), node("/c", 7)];
        let tasks = [
            timer("/a", Some(1), 4),
            timer("/b", Some(2), 8),
            timer("/c", Some(6), 40),
        ];
        let plan = derive_levels(&input(&free, &[], &tasks, 2));
        assert_eq!(members(&plan), vec![vec!["/a", "/b"], vec!["/c"]]);
        assert_eq!(plan.method, LevelMethod::ResponseTime);

        // The same jobs under the naive banding: B misses.
        let jobs: Vec<Job> = tasks
            .iter()
            .enumerate()
            .map(|(k, t)| job_of(t, k).unwrap())
            .collect();
        assert_eq!(response_time(&jobs, 1, 1, 2), None, "B misses behind C");
        assert_eq!(response_time(&jobs, 0, 0, 0), Some(1));
        // And every job of the chosen banding meets its deadline.
        assert_eq!(response_time(&jobs, 0, 0, 1), Some(3));
        assert_eq!(response_time(&jobs, 1, 0, 1), Some(3));
        assert_eq!(response_time(&jobs, 2, 2, 2), Some(14));
        // One band would let C block A past its 4 us.
        assert_eq!(response_time(&jobs, 0, 0, 2), None);

        // Without C the same image falls back to the capped merge.
        let blind = [
            timer("/a", None, 4),
            timer("/b", None, 8),
            timer("/c", None, 40),
        ];
        let plan = derive_levels(&input(&free, &[], &blind, 2));
        assert_eq!(members(&plan), vec![vec!["/a"], vec!["/b", "/c"]]);
        assert_eq!(plan.method, LevelMethod::Capped);
    }

    /// Issue 0259: work this call cannot see (unpinned nodes beside placed
    /// ones) may preempt on its core, so the analysed merge cannot bound the
    /// bands and the levels come from rules 1 and 3, exactly as for a pin.
    #[test]
    fn floating_interference_keeps_the_analysed_merge_off() {
        let free = [node("/a", 5), node("/b", 6), node("/c", 7)];
        let tasks = [
            timer("/a", Some(1), 100),
            timer("/b", Some(1), 200),
            timer("/c", Some(1), 400),
        ];
        let mut inp = input(&free, &[], &tasks, 8);
        assert_eq!(derive_levels(&inp).method, LevelMethod::ResponseTime);
        inp.core = Some(1);
        inp.floating_interference = true;
        let plan = derive_levels(&inp);
        assert_eq!(plan.method, LevelMethod::PerRank);
        assert_eq!(plan.levels.len(), 3);
        assert_eq!(plan.core, Some(1));
    }

    /// The analysed merge also merges below the cap: it finds the fewest
    /// levels, and an all-feasible single band is one level.
    #[test]
    fn the_analysed_merge_finds_the_fewest_levels() {
        let free = [node("/a", 5), node("/b", 6), node("/c", 7)];
        let tasks = [
            timer("/a", Some(1), 100),
            timer("/b", Some(1), 200),
            timer("/c", Some(1), 400),
        ];
        let plan = derive_levels(&input(&free, &[], &tasks, 8));
        assert_eq!(plan.levels.len(), 1);
        assert_eq!(plan.method, LevelMethod::ResponseTime);
        assert_eq!(level_budget(&plan), vec!["/b", "/c"]);
        assert!(
            plan.degradations[0]
                .reason
                .contains("response-time analysis"),
            "{}",
            plan.degradations[0].reason
        );
    }

    /// An authored tier is a pin: its members form one band at its own
    /// priority, the free nodes are banded around it, and a free band never
    /// spans it.
    #[test]
    fn an_authored_tier_is_a_pin() {
        let free = [node("/a", 5), node("/c", 9)];
        let pinned = [PinnedBand {
            tier: "mid".to_string(),
            priority: 7,
            members: vec![node("/m1", 7), node("/m2", 8)],
        }];
        let plan = derive_levels(&input(&free, &pinned, &[], 2));
        // Cap 2 cannot be met: the pin splits the free nodes.
        assert_eq!(
            members(&plan),
            vec![vec!["/a"], vec!["/m1", "/m2"], vec!["/c"]]
        );
        assert_eq!(plan.levels[1].tiers, vec!["mid".to_string()]);
        assert!(
            plan.degradations
                .iter()
                .any(|d| d.dim == "level-budget" && d.reason.contains("cannot be met")),
            "{:?}",
            plan.degradations
        );
        // No free member lost a rank; the pin's members are the tier's report.
        assert!(
            level_budget(&plan)
                .iter()
                .all(|n| n.starts_with("max_levels"))
        );
    }

    /// Rule 8: on a time-slicing kernel two levels at one kernel priority are
    /// one level.
    #[test]
    fn two_levels_at_one_kernel_priority_merge_on_a_time_slicing_kernel() {
        let free = [node("/a", 3), node("/b", 2)];
        let pinned = [PinnedBand {
            tier: "ctl".to_string(),
            priority: 3,
            members: vec![node("/p", 3)],
        }];
        let mut inp = input(&free, &pinned, &[], 8);
        inp.low_number_is_high = false; // FreeRTOS: bigger is more urgent
        let fifo = derive_levels(&inp);
        assert_eq!(fifo.levels.len(), 3, "FIFO kernel: two threads at 3");
        inp.time_slicing = true;
        let sliced = derive_levels(&inp);
        assert_eq!(members(&sliced), vec![vec!["/p", "/a"], vec!["/b"]]);
        assert_eq!(sliced.levels[0].tiers, vec!["ctl".to_string()]);
        assert!(sliced.degradations.iter().any(|d| d.dim == "level"));
    }

    /// An unanalysable callback goes to the least urgent rank of its level.
    #[test]
    fn an_unanalysable_callback_dispatches_at_its_levels_least_urgent_rank() {
        let free = [node("/a", 5), node("/b", 6)];
        let mut sub = timer("/a", None, 10);
        sub.callback = "/a/cmd".to_string();
        sub.kind = CallbackKind::Subscription;
        sub.release = ReleaseKind::Unanalysable;
        sub.source = ReleaseSource::None;
        sub.period_us = None;
        sub.deadline_us = None;
        let tasks = [sub];
        let plan = derive_levels(&input(&free, &[], &tasks, 1));
        assert_eq!(plan.levels[0].best_effort, vec![("/a/cmd".to_string(), 1)]);
    }

    /// The platform defaults of RFC-0108 D6.
    #[test]
    fn the_platform_defaults_follow_d6() {
        let z = PriorityPlan::for_target("zephyr");
        assert_eq!(default_max_levels("zephyr", &z), 8);
        assert_eq!(
            default_max_levels("freertos", &PriorityPlan::for_target("freertos")),
            4
        );
        let t = PriorityPlan::for_target("threadx");
        assert_eq!(default_max_levels("threadx", &t), t.app.width() as u16);
        assert!(time_slices_equal_priorities("freertos"));
        assert!(!time_slices_equal_priorities("zephyr"));
    }
}
