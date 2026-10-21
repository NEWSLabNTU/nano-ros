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
    priority_plan::PriorityPlan,
    rtos_realizer::{Degradation, realize_rtos, sched_caps_for},
};

/// The outcome of deriving a schedule from the contract layer.
#[derive(Debug, Default)]
pub struct DerivedSchedule {
    /// Derived tier definitions, keyed by tier name: one per derived level
    /// (`derived-<node>`, or `derived-level-<k>` for a level of several
    /// nodes; phase-509 P5). These are NEW
    /// tiers, for nodes whose groups name no tier at all; the caller MERGES
    /// them into whatever the model authored (see [`ADDS`](DerivedSchedule)).
    pub tiers: BTreeMap<String, TierDef>,
    /// One override per derived node, binding its callback groups to its
    /// level's tier.
    pub overrides: Vec<NodeOverride>,
    /// issue 1426 — placements ALLOCATED for tiers the author NAMED and left
    /// unplaced, keyed by the authored tier name.
    ///
    /// The caller installs each on the `[tiers.<name>]` it already built from
    /// the model, so the head facts (`class`, `period_us`, `deadline_us`, …)
    /// stay AUTHORED and only the address is allocated. It is a separate field
    /// from [`Self::tiers`] for exactly that reason: a whole `TierDef` here
    /// would be this function re-deciding facts it was not asked about.
    pub placements: BTreeMap<String, TierRtosSpec>,
    /// Every guarantee weakening the realizer recorded (fail-loud — the caller
    /// MUST surface these).
    pub degradations: Vec<Degradation>,
    /// Nodes that ranked but declare no callback groups — they stay on the
    /// default tier (advisory-derived, not authored). One human-readable note
    /// per node; the caller surfaces them.
    pub groupless_notes: Vec<String>,
    /// issue 1426 — every authored placement that WON over one the contract
    /// implied. Authored wins, and it may not win silently.
    pub shadowed: Vec<ShadowedPlacement>,
    /// Every node the contract ranked, as the realizer placed it in the plan
    /// (priority, period, deadline). What a record of the derivation shows
    /// for "why this number": the tiers above are built from these.
    pub realized: Vec<crate::rtos_realizer::RealizedNode>,
    /// RFC-0079 section 6 - the nodes allocated above the transport band
    /// because their tier names it (`above = "transport"`, issue 1967;
    /// [`lift_above_transport`]). Their `realized` priority is the lifted
    /// one.
    pub above_transport: Vec<String>,
    /// Where the supervisor that reacts to a missed supervised deadline
    /// runs: above the transport, one beyond the most urgent lifted node,
    /// or adjacent to the transport when nothing was lifted. `None` when
    /// the contract supervises nothing, or the kernel had no priority left
    /// for it (then a degradation says so).
    pub supervisor_priority: Option<i64>,
    /// phase-509 P4 (RFC-0108 D1) - the task record of every callback the
    /// model names: release kind, T or T_min, D, C, and a server's Q/P
    /// ([`crate::task_record`]). On an image whose contract supervises
    /// nothing, each unanalysable one is also a `unanalysable` degradation;
    /// on one that supervises a deadline it is refused before this runs
    /// ([`crate::task_record::check_model`]).
    pub tasks: Vec<crate::task_record::TaskRecord>,
    /// phase-509 P5 (RFC-0108 D4) - the preemption levels: the derived nodes
    /// banded under the image's cap, the authored tiers as pins. `None` when
    /// nothing ranked. Each derived level is one tier in [`Self::tiers`]
    /// (`derived-<node>` when it holds one node, `derived-level-<k>` when it
    /// holds several) until the level runners (phase-509 P9) run levels.
    pub levels: Option<crate::levels::LevelPlan>,
}

/// The node FQNs whose contract supervises a deadline: a node path
/// (`contracts.node_paths."<node>/<path>"`) carrying `supervise:`.
pub fn supervised_nodes(model: &SystemModel) -> std::collections::BTreeSet<String> {
    model
        .contracts
        .node_paths
        .iter()
        .filter(|(_, p)| p.supervise.is_some())
        .filter_map(|(k, _)| k.rsplit_once('/').map(|(node, _)| node.to_string()))
        .collect()
}

/// RFC-0079 section 6: does this authored tier NAME the transport band
/// (`[tiers.<name>] above = "transport"`)?
///
/// Issue 1967. The key is rlm's `TierDef::above` (`ReservedBand`, a closed
/// set whose one member is `transport`; rlm v0.1.67). rlm refuses a misspelt
/// band at parse, so a tier that reaches this either named the transport or
/// named nothing.
fn names_the_transport(t: &ros_launch_manifest_sched::TierDef) -> bool {
    t.above == Some(ros_launch_manifest_sched::ReservedBand::Transport)
}

/// The authored tiers that name the transport band AND ask for an allocated
/// priority on this target (issue 1426 rule 2: no platform sub-table at
/// all). A tier with an authored priority keeps it (rule 1, `above` does
/// not move a number the author wrote); one placed for other boards only is
/// rule 3's error, not a request.
pub fn tiers_named_above_transport(
    model: &SystemModel,
    target_rtos: &str,
) -> std::collections::BTreeSet<String> {
    model
        .execution
        .tiers
        .iter()
        .filter(|(_, t)| {
            names_the_transport(t) && t.platform(target_rtos).is_none() && !authors_any_placement(t)
        })
        .map(|(n, _)| n.clone())
        .collect()
}

/// RFC-0079 section 6: crossing into the transport band requires naming it,
/// and the name is `above = "transport"` on the tier. Nothing else crosses:
/// a node whose contract supervises a deadline (`supervise:`) stays where
/// its rank puts it, which may be below the transport (issue 1967; until
/// then #2312 lifted such a node implicitly).
///
/// `lift` is the set of nodes to allocate ABOVE the reserved transport band
/// (the most urgent members of the tiers that name it, see
/// [`derive_tiers_with_named`]). They keep their relative rank: the least
/// urgent takes the priority adjacent to the transport, each more urgent
/// rank the next one out ([`PriorityPlan::slots_above_transport`]); equal
/// ranks share. No room for the ranks: nothing is lifted, and a degradation
/// says so.
///
/// `supervised` is the set of nodes whose contract supervises a deadline.
/// When it is not empty, one priority above the transport is reserved for
/// the supervisor that reacts to a miss: beyond the most urgent lifted rank
/// when something was lifted, else the one adjacent to the transport. The
/// supervisor is the reaction path and publishes on its own, so it runs
/// above the transport whether or not the executor it supervises does. A
/// supervised node left below the transport is recorded (a note: its
/// supervised budget now includes whatever the transport runs meanwhile),
/// and is not moved.
///
/// Returns the lifted node names and the supervisor's priority.
pub fn lift_above_transport(
    nodes: &mut [crate::rtos_realizer::RealizedNode],
    lift: &std::collections::BTreeSet<String>,
    supervised: &std::collections::BTreeSet<String>,
    plan: &PriorityPlan,
    low_number_is_high: bool,
    degradations: &mut Vec<Degradation>,
) -> (Vec<String>, Option<i64>) {
    let Some(transport) = plan.reserved.get("transport").copied() else {
        return (Vec::new(), None);
    };
    let slots = plan.slots_above_transport();
    let candidates: Vec<usize> = (0..nodes.len())
        .filter(|&i| lift.contains(&nodes[i].name))
        .collect();
    // Distinct ranks, MOST urgent first.
    let mut ranks: Vec<i64> = candidates.iter().map(|&i| nodes[i].priority).collect();
    ranks.sort_unstable();
    ranks.dedup();
    if !low_number_is_high {
        ranks.reverse();
    }
    let lifted: Vec<usize> = if slots.len() < ranks.len() {
        for &i in &candidates {
            degradations.push(Degradation {
                node: nodes[i].name.clone(),
                dim: "priority",
                reason: format!(
                    "its tier names `above = \"transport\"` - but the kernel has {} \
                     priorit{} above the transport for {} rank(s); it stays in pool.app \
                     [{}, {}], below the transport",
                    slots.len(),
                    if slots.len() == 1 { "y" } else { "ies" },
                    ranks.len(),
                    plan.app.lo,
                    plan.app.hi
                ),
            });
        }
        Vec::new()
    } else {
        // ranks[k] (k-th most urgent) -> slots[n - 1 - k] (the least urgent
        // rank adjacent to the transport).
        let n = ranks.len();
        for &i in &candidates {
            let k = ranks
                .iter()
                .position(|&r| r == nodes[i].priority)
                .unwrap_or(0);
            nodes[i].priority = slots[n - 1 - k];
        }
        candidates
    };
    let used = if lifted.is_empty() { 0 } else { ranks.len() };
    let watched: Vec<usize> = (0..nodes.len())
        .filter(|&i| supervised.contains(&nodes[i].name))
        .collect();
    let supervisor = if watched.is_empty() {
        None
    } else {
        slots.get(used).copied()
    };
    for &i in &watched {
        if supervisor.is_none() {
            degradations.push(Degradation {
                node: nodes[i].name.clone(),
                dim: "priority",
                reason: "its contract supervises a deadline, and no priority remains above \
                         the transport for the supervisor that reacts to a miss"
                    .to_string(),
            });
        }
        if !lifted.contains(&i) {
            degradations.push(Degradation {
                node: nodes[i].name.clone(),
                dim: "priority",
                reason: format!(
                    "note: its contract supervises a deadline, and its tier does not name \
                     `above = \"transport\"`, so its executor runs at {} below the \
                     transport [{}, {}]: the supervised budget includes whatever the \
                     transport runs meanwhile{}",
                    nodes[i].priority,
                    transport.lo,
                    transport.hi,
                    match supervisor {
                        Some(p) => format!("; the supervisor runs at {p}, above the transport"),
                        None => String::new(),
                    }
                ),
            });
        }
    }
    (
        lifted.iter().map(|&i| nodes[i].name.clone()).collect(),
        supervisor,
    )
}

/// issue 1426 — an authored priority that took precedence over an allocated one.
///
/// Not a [`Degradation`]: nothing was weakened, a decision was made. But
/// "authored wins" is only a defensible rule while the person who authored the
/// number can see what it beat, so every caller prints one line per row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowedPlacement {
    /// The node whose rank was not used.
    pub node: String,
    /// The authored tier it is bound to.
    pub tier: String,
    /// What `[tiers.<tier>.<rtos>] priority` says.
    pub authored: i64,
    /// What the contract's rank would have allocated for it.
    pub allocated: i64,
}

/// issue 1426 — does any node carrying callback groups lack an authored
/// placement for this target? THE predicate that decides whether a derivation
/// runs, in one spelling.
///
/// Before this, three sites asked `model.execution.tiers.is_empty()` instead —
/// `cmd::codegen_system`, `codegen::entry::derive_entry_tiers` and the
/// `nros::main!` macro — and that question is about the TABLE where the rule is
/// about a FACT. Precedence, stated once and per fact:
///
/// 1. `[tiers.<name>.<rtos>]` authored for this target ⇒ **AUTHORED**. Never
///    overwritten; a rank it beat is recorded in
///    [`DerivedSchedule::shadowed`].
/// 2. `[tiers.<name>]` authored with **no** platform sub-table at all ⇒
///    **ALLOCATED** for that tier ([`DerivedSchedule::placements`]). Naming a
///    tier is how a binding gets written (`group_tiers = { main = "ctrl" }`);
///    it says nothing about a priority, and RFC-0079 is that a priority is
///    allocated rather than authored. So an unplaced tier is a REQUEST, which
///    is also why this needs no new key in the `[tiers.*]` schema.
/// 3. `[tiers.<name>]` authored with sub-tables for OTHER targets and not this
///    one ⇒ neither. The author did place it, for a different board, and
///    `resolve_tiers` raises `MissingRtosSpec` exactly as before. Reading that
///    as a request would silence a real mistake.
/// 4. the group names no tier (`DEFAULT_TIER`) ⇒ **ALLOCATED** on a derived
///    level ([`DerivedSchedule::tiers`]): phase-509 P5 bands these nodes into
///    preemption levels (RFC-0108 D4, [`crate::levels`]), one derived tier per
///    level, `derived-<node>` for a level of one node.
///
/// Rule 4 is what the old table-shaped guard got wrong. A group at
/// `DEFAULT_TIER` is a node the author wrote nothing about, and whether it gets
/// a derived placement cannot depend on whether some OTHER node's tier was
/// authored — which is what `tiers.is_empty()` made it depend on. Measured on
/// the phase-459 W0 fixture: one component binding to one authored
/// `[tiers.ctrl.zephyr]` left the other three on the synthesised default tier
/// at Zephyr priority 0, i.e. ABOVE the transport that feeds them, with their
/// rate contracts unread and nothing printed.
pub fn placement_is_unauthored(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
) -> bool {
    callback_groups
        .values()
        .flatten()
        .any(|g| match model.execution.tiers.get(&g.tier) {
            // Rule 4 (and an undeclared name, which `apply_model_execution`
            // refuses before this ever matters).
            None => true,
            // Rules 1-3.
            Some(t) => t.platform(target_rtos).is_none() && !authors_any_placement(t),
        })
}

/// issue 1426 rule 2 — one member's bid for the single priority an authored,
/// unplaced tier gets: `(node FQN, allocated priority, preempt threshold)`.
///
/// A tier is one thread, so several of these compete and the most urgent wins
/// (see the loop at the end of [`derive_tiers_from_contracts`]). Named because
/// the tuple is the whole reason clippy calls the map's type complex, and a
/// name is the answer it asks for.
type AllocationCandidate<'a> = (&'a str, i64, Option<i64>);

/// Has this tier a platform sub-table for ANY target? The discriminator between
/// rules 2 and 3 above — see [`placement_is_unauthored`].
fn authors_any_placement(t: &ros_launch_manifest_sched::TierDef) -> bool {
    t.posix.is_some()
        || t.freertos.is_some()
        || t.zephyr.is_some()
        || t.threadx.is_some()
        || t.nuttx.is_some()
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
///
/// issue 1426 — this runs WHENEVER [`placement_is_unauthored`] holds, not only
/// on an empty tier table, and it never overwrites an authored fact. Read that
/// function's doc for the precedence; the three outputs it decides between are
/// [`DerivedSchedule::tiers`] (rule 4), [`DerivedSchedule::placements`]
/// (rule 2) and [`DerivedSchedule::shadowed`] (rule 1).
pub fn derive_tiers_from_contracts(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
) -> DerivedSchedule {
    derive_tiers_in_plan(
        model,
        target_rtos,
        callback_groups,
        &PriorityPlan::for_target(target_rtos),
    )
}

/// [`derive_tiers_from_contracts`], allocating out of a plan the CALLER
/// resolved rather than the one [`PriorityPlan::for_target`] projects from a
/// tier key.
///
/// Issue 1508. For Zephyr, `for_target` is the Kconfig DEFAULTS projection
/// (15 preemptive priorities, one transport task at band 200 -> pool
/// `[5, 14]`), and an image is free to differ: measured, an image with
/// `CONFIG_NUM_PREEMPT_PRIORITIES=32` reserves `[7, 7]` and owns `[8, 31]`,
/// and one with `CONFIG_NROS_ZENOH_READ_PRIORITY=100` reserves `[4, 9]` and
/// owns `[10, 14]`. Against either, the projection's rank 0 (5) is MORE urgent
/// than the transport - issue 0623's inversion. A road that has the image's
/// `.config` in hand (the C/C++ entry and the Zephyr module bake both run at
/// cmake configure time, after Kconfig) resolves the image's own plan with
/// [`PriorityPlan::from_zephyr_dotconfig`] and passes it here.
pub fn derive_tiers_in_plan(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    priority_plan: &PriorityPlan,
) -> DerivedSchedule {
    derive_tiers_with_named(
        model,
        target_rtos,
        callback_groups,
        priority_plan,
        &tiers_named_above_transport(model, target_rtos),
    )
}

/// [`derive_tiers_in_plan`] with the tiers that name the transport band
/// given (issue 1967; [`tiers_named_above_transport`] reads them off the
/// model). A tier is one thread at one priority, its most urgent member's
/// (issue 1426 rule 2), so it is that member's rank that is lifted: every
/// member at the tier's most urgent rank joins [`lift_above_transport`]'s
/// set, and the others keep their pool ranks, which no thread runs at.
pub fn derive_tiers_with_named(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    priority_plan: &PriorityPlan,
    named_above_transport: &std::collections::BTreeSet<String>,
) -> DerivedSchedule {
    derive_tiers_with_wcet(
        model,
        target_rtos,
        callback_groups,
        priority_plan,
        named_above_transport,
        None,
    )
}

/// [`derive_tiers_with_named`] with an RFC-0078 measurement profile: the task
/// records carry C where it declares a bound, and the level derivation's
/// analysed merge (RFC-0108 D4 rule 2) engages when every derived callback
/// has one. The bake roads select no profile yet, so they pass `None` and the
/// levels come from rules 1 and 3.
pub fn derive_tiers_with_wcet(
    model: &SystemModel,
    target_rtos: &str,
    callback_groups: &BTreeMap<String, Vec<CallbackGroupDecl>>,
    priority_plan: &PriorityPlan,
    named_above_transport: &std::collections::BTreeSet<String>,
    wcet: Option<&crate::wcet::WcetProfile>,
) -> DerivedSchedule {
    // phase-509 P4 (RFC-0108 D1) - the task record, and its report on an
    // image that supervises nothing. Before the rank, so an image with
    // nothing to rank still reports an unbounded input.
    let tasks = crate::task_record::task_records(model, wcet);
    let unanalysable = if supervised_nodes(model).is_empty() {
        crate::task_record::unanalysable_degradations(&tasks)
    } else {
        Vec::new()
    };
    let input = mapper_input_from_model(model);
    let ranked = chain_aware_rank(&input);
    if ranked.items.is_empty() {
        return DerivedSchedule {
            degradations: unanalysable,
            tasks,
            ..Default::default()
        };
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
    // phase-459 W4 (issue 1427) - the priority is ALLOCATED out of the board's
    // address plan, not read off the rank. Issue 1508 - the plan is the
    // caller's: `derive_tiers_from_contracts` passes the tier key's projection,
    // a road holding the image's `.config` passes the image's own.
    let mut plan = realize_rtos(&ranked, &input, &caps, priority_plan);
    // RFC-0079 section 6 - a tier that names the transport band is
    // allocated above it; nothing else crosses (issue 1967).
    let mut lift = std::collections::BTreeSet::new();
    for tier in named_above_transport {
        if !priority_plan.reserved.contains_key("transport") {
            plan.degradations.push(Degradation {
                node: format!("[tiers.{tier}]"),
                dim: "priority",
                reason: format!(
                    "the tier names `above = \"transport\"`, but the plan for this \
                     target ({target_rtos:?}) reserves no transport band: it is \
                     allocated in pool.app [{}, {}]",
                    priority_plan.app.lo, priority_plan.app.hi
                ),
            });
            continue;
        }
        let members: Vec<&crate::rtos_realizer::RealizedNode> = plan
            .nodes
            .iter()
            .filter(|n| {
                callback_groups
                    .get(bare(&n.name))
                    .is_some_and(|gs| gs.iter().any(|g| &g.tier == tier))
            })
            .collect();
        let lead = if caps.low_number_is_high {
            members.iter().map(|n| n.priority).min()
        } else {
            members.iter().map(|n| n.priority).max()
        };
        if let Some(lead) = lead {
            lift.extend(
                members
                    .iter()
                    .filter(|n| n.priority == lead)
                    .map(|n| n.name.clone()),
            );
        }
    }
    let (above_transport, supervisor_priority) = lift_above_transport(
        &mut plan.nodes,
        &lift,
        &supervised_nodes(model),
        priority_plan,
        caps.low_number_is_high,
        &mut plan.degradations,
    );
    plan.degradations.extend(unanalysable);
    let mut out = DerivedSchedule {
        degradations: plan.degradations.clone(),
        realized: plan.nodes.clone(),
        above_transport,
        supervisor_priority,
        tasks,
        ..Default::default()
    };

    // issue 1426 rule 2 — authored tier name -> the members whose ranks are
    // candidates for the one priority that tier's single thread gets.
    let mut unplaced: BTreeMap<String, Vec<AllocationCandidate<'_>>> = BTreeMap::new();

    // phase-509 P5 (RFC-0108 D4) - rule 4 no longer makes a tier per node:
    // the nodes no tier binds are collected here, banded into levels below,
    // and each derived level is one tier. Authored tiers (rules 1 and 2) are
    // the levels' pins: `pins` keeps each one's members.
    let mut free: Vec<(&crate::rtos_realizer::RealizedNode, Vec<&CallbackGroupDecl>)> = Vec::new();
    let mut pins: BTreeMap<String, (Option<i64>, Vec<&crate::rtos_realizer::RealizedNode>)> =
        BTreeMap::new();

    for n in &plan.nodes {
        let node = bare(&n.name).to_string();
        // A ranked node with no declared callback groups has nothing for the
        // gating executor to bind — it stays on the default tier (loud note).
        let Some(groups) = callback_groups.get(&node).filter(|g| !g.is_empty()) else {
            out.groupless_notes.push(n.name.clone());
            continue;
        };

        // issue 1426 — split this node's groups by what the author said about
        // the tier each one NAMES. The four rules are stated once, on
        // `placement_is_unauthored`; this is where they are applied.
        let mut unbound: Vec<&CallbackGroupDecl> = Vec::new();
        for g in groups.iter() {
            match model.execution.tiers.get(&g.tier) {
                // Rule 4 — the group names no tier (`DEFAULT_TIER`), or names
                // one `apply_model_execution` will refuse as undeclared.
                None => unbound.push(g),
                Some(t) => {
                    if let Some(authored) = t.platform(target_rtos) {
                        // Rule 1 — AUTHORED wins, and says what it beat.
                        out.shadowed.push(ShadowedPlacement {
                            node: node.clone(),
                            tier: g.tier.clone(),
                            authored: authored.priority,
                            allocated: n.priority,
                        });
                        let pin = pins
                            .entry(g.tier.clone())
                            .or_insert((Some(authored.priority), Vec::new()));
                        if !pin.1.iter().any(|m| m.name == n.name) {
                            pin.1.push(n);
                        }
                    } else if authors_any_placement(t) {
                        // Rule 3 — placed, for another board. `resolve_tiers`
                        // raises `MissingRtosSpec`; silencing it here would
                        // turn a real mistake into a derived number.
                    } else {
                        // Rule 2 — NAMED and unplaced: an allocation request.
                        unplaced.entry(g.tier.clone()).or_default().push((
                            n.name.as_str(),
                            n.priority,
                            n.preempt_threshold,
                        ));
                        let pin = pins.entry(g.tier.clone()).or_insert((None, Vec::new()));
                        if !pin.1.iter().any(|m| m.name == n.name) {
                            pin.1.push(n);
                        }
                    }
                }
            }
        }
        if !unbound.is_empty() {
            free.push((n, unbound));
        }
    }

    // Issue 1285 follow-up. `target_rtos` is a tier KEY (the output of
    // `tier_rtos_key`, or `""` for a board with no RTOS), so it is matched
    // EXACTLY. For a no-RTOS board a derived level has no
    // `[tiers.<name>.<rtos>]` sub-table to live in: such a node stays on the
    // default tier, and the degradation record says so.
    if !free.is_empty() && !rtos_slot_exists(target_rtos) {
        for (n, _) in &free {
            out.degradations.push(Degradation {
                node: n.name.clone(),
                dim: "tier",
                reason: format!(
                    "the target (tier key {target_rtos:?}) has no RTOS family, so a derived \
                     tier has no `[tiers.<name>.<rtos>]` sub-table and no task to run \
                     on; the node stays on the default tier"
                ),
            });
        }
        free.clear();
    }

    // phase-509 P5 (RFC-0108 D4) - the levels: the free nodes banded, the
    // authored tiers pinned, the cap the image's or the platform's.
    let supervised = supervised_nodes(model);
    let level_nodes: Vec<crate::levels::LevelNode> = free
        .iter()
        .map(|(n, _)| crate::levels::LevelNode {
            node: n.name.clone(),
            priority: n.priority,
            protected: if supervised.contains(&n.name) {
                Some("supervised")
            } else if n.deadline_us.is_some() {
                Some("deadline")
            } else {
                None
            },
        })
        .collect();
    let most_urgent = |ps: &mut dyn Iterator<Item = i64>| {
        if caps.low_number_is_high {
            ps.min()
        } else {
            ps.max()
        }
    };
    let pinned: Vec<crate::levels::PinnedBand> = pins
        .iter()
        .filter_map(|(tier, (authored, members))| {
            let priority =
                authored.or_else(|| most_urgent(&mut members.iter().map(|m| m.priority)))?;
            Some(crate::levels::PinnedBand {
                tier: tier.clone(),
                priority,
                members: members
                    .iter()
                    .map(|m| crate::levels::LevelNode {
                        node: m.name.clone(),
                        priority: m.priority,
                        protected: None,
                    })
                    .collect(),
            })
        })
        .collect();
    let (max_levels, cap_source) = match priority_plan.max_levels {
        Some(n) => (n, "image"),
        None => (
            crate::levels::default_max_levels(target_rtos, priority_plan),
            "platform",
        ),
    };
    let levels = crate::levels::derive_levels(&crate::levels::LevelInput {
        free: &level_nodes,
        pinned: &pinned,
        tasks: &out.tasks,
        max_levels,
        cap_source,
        low_number_is_high: caps.low_number_is_high,
        time_slicing: crate::levels::time_slices_equal_priorities(target_rtos),
        ranked_priorities: &plan.nodes.iter().map(|n| n.priority).collect::<Vec<_>>(),
    });
    out.degradations.extend(levels.degradations.iter().cloned());

    for level in &levels.levels {
        let members: Vec<&(&crate::rtos_realizer::RealizedNode, Vec<&CallbackGroupDecl>)> = level
            .derived_members()
            .filter_map(|m| free.iter().find(|(n, _)| n.name == m.node))
            .collect();
        if members.is_empty() {
            continue;
        }
        // D4 rule 8 put these nodes on an authored tier's level: bind their
        // groups to that tier.
        let tier_name = match level.tiers.first() {
            Some(t) => t.clone(),
            None => {
                let name = match members.as_slice() {
                    // One node: the tier keeps the node's name, as before.
                    [(n, _)] => format!("derived{}", n.name.replace('/', "-")),
                    _ => format!("derived-level-{}", level.index),
                };
                out.tiers.insert(
                    name.clone(),
                    level_tier(&members, target_rtos, &mut out.degradations),
                );
                name
            }
        };
        for (n, groups) in &members {
            out.overrides.push(NodeOverride {
                name: bare(&n.name).to_string(),
                callback_groups: groups
                    .iter()
                    .map(|g| CallbackGroupOverride {
                        id: g.id.clone(),
                        tier: tier_name.clone(),
                    })
                    .collect(),
            });
        }
    }
    out.levels = Some(levels);

    // issue 1426 rule 2 — one priority per NAMED, unplaced tier.
    //
    // A tier is one thread, so it has one address however many nodes bind to
    // it: it takes the MOST URGENT rank among its members, because the
    // alternative is running an urgent node at a relaxed node's priority. When
    // the members do not agree, the author collapsed two ranks into one tier
    // and lost the rate-monotonic split between them — that is a weakened
    // guarantee, so it is a recorded `Degradation` and not a comment here.
    for (tier, mut members) in unplaced {
        // Most urgent first, in the BOARD's direction. `RealizedNode::priority`
        // is already flipped for `low_number_is_high`, so the comparison has to
        // ask the caps rather than assume a direction.
        members.sort_by(|a, b| {
            if caps.low_number_is_high {
                a.1.cmp(&b.1)
            } else {
                b.1.cmp(&a.1)
            }
        });
        let (leader, priority, preempt_threshold) = members[0];
        // phase-509 P1 (RFC-0108 D4 rule 5): one degradation per member whose
        // rank the tier does not keep, not only the first. A node with two
        // groups on this tier is one member, so it is reported once; the
        // stable sort keeps the order the members were met in among equals.
        let mut reported: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for &(other, relaxed, _) in members.iter().skip(1) {
            if relaxed == priority || !reported.insert(other) {
                continue;
            }
            out.degradations.push(Degradation {
                node: other.to_string(),
                dim: "tier",
                reason: format!(
                    "`[tiers.{tier}]` states no priority for this board, so one is \
                     allocated from the contract - but its members do not share a \
                     rank: `{leader}` would take {priority} and this node {relaxed}. \
                     One tier is one thread at one priority, so the tier takes the \
                     more urgent of the two and the rate-monotonic split between \
                     them is lost. Name a second tier to keep it."
                ),
            });
        }
        if !rtos_slot_exists(target_rtos) {
            out.degradations.push(Degradation {
                node: leader.to_string(),
                dim: "tier",
                reason: format!(
                    "the target (tier key {target_rtos:?}) has no RTOS family, so the \
                     unplaced `[tiers.{tier}]` has no `[tiers.{tier}.<rtos>]` sub-table \
                     to allocate a priority into; `resolve_tiers` will refuse it"
                ),
            });
            continue;
        }
        out.placements.insert(
            tier,
            TierRtosSpec {
                priority,
                preempt_threshold,
                // Everything else stays AUTHORED on the `[tiers.<name>]`
                // head — see `DerivedSchedule::placements`. Spelled out rather
                // than `..Default::default()`, because a defaulted `priority`
                // would be a silent 0 and 0 is a legal Zephyr priority.
                stack_bytes: None,
                time_slice_us: None,
                sched_class: None,
                core: None,
                deadline_us: None,
                budget_us: None,
                period_us: None,
            },
        );
    }
    out
}

/// The tier one derived level runs as until the level runners land
/// (phase-509 P9): one thread at the band's most urgent priority. A level of
/// one node is exactly the tier rule 4 always made for it. A level of several
/// takes the tightest period and deadline among them; a per-node sporadic
/// budget cannot be enforced on a shared thread, so it is dropped with a
/// `budget` degradation naming the node.
fn level_tier(
    members: &[&(&crate::rtos_realizer::RealizedNode, Vec<&CallbackGroupDecl>)],
    target_rtos: &str,
    degradations: &mut Vec<Degradation>,
) -> TierDef {
    let lead = members[0].0;
    let one = members.len() == 1;
    let min_of = |f: &dyn Fn(&crate::rtos_realizer::RealizedNode) -> Option<u64>| {
        members.iter().filter_map(|(n, _)| f(n)).min()
    };
    let deadline_us = min_of(&|n| n.deadline_us);
    let period_us = min_of(&|n| n.period_us);
    let budget_us = if one {
        lead.budget_us
    } else {
        for (n, _) in members {
            if let Some(b) = n.budget_us {
                degradations.push(Degradation {
                    node: n.name.clone(),
                    dim: "budget",
                    reason: format!(
                        "its sporadic budget of {b}us is not enforced: it shares a \
                         derived level with other nodes (RFC-0108 D4), and one budget \
                         on the level's thread would throttle them too"
                    ),
                });
            }
        }
        None
    };
    let spec = TierRtosSpec {
        priority: lead.priority,
        stack_bytes: None,
        preempt_threshold: lead.preempt_threshold,
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
            if deadline_us.is_some() || budget_us.is_some() {
                "real_time"
            } else {
                "best_effort"
            }
            .to_string(),
        ),
        period_us,
        budget_us,
        deadline_us,
        core: lead.core,
        ..Default::default()
    };
    let slot = match target_rtos {
        "zephyr" => &mut def.zephyr,
        "freertos" => &mut def.freertos,
        "threadx" => &mut def.threadx,
        "nuttx" => &mut def.nuttx,
        _ => &mut def.posix,
    };
    *slot = Some(spec);
    def
}

/// Does `target_rtos` name an RTOS family with a `[tiers.<name>.<rtos>]`
/// sub-table to install a placement into? The same question the `slot` match
/// above answers for rule 4; a board with no family (tier key `""`) gets
/// neither, and the realizer's degradation record says so.
fn rtos_slot_exists(target_rtos: &str) -> bool {
    matches!(
        target_rtos,
        "zephyr" | "freertos" | "threadx" | "nuttx" | "posix" | "native"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ros_launch_manifest_model::{
        Contracts, NodeInstance, PathContract, PubContract, Structure, SystemModel, TopicWiring,
    };
    use ros_launch_manifest_sched::EffectiveTrigger;

    use super::*;
    use crate::{CallbackGroupDecl, resolve_tiers};

    /// A contract-only model: control is a tight 5 ms / 100 Hz loop, telem a
    /// slack 100 ms / 10 Hz loop, NO `execution.tiers`.
    ///
    /// phase-495 W1 - each timer path carries its `trigger` (rlm v0.1.37,
    /// design issue #52): the timer's rate lives on the path, and an empty
    /// `input` says nothing about what fires it. phase-495 W2 - the
    /// publishers' rate PROMISES that W1 kept beside the timers are gone,
    /// because nothing derives a period from them any more: the schedule
    /// below is read off the triggers alone.
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
                trigger: Some(EffectiveTrigger::Timer { rate_hz: 100.0 }),
                max_latency_ms: Some(5.0),
                ..Default::default()
            },
        );
        node_paths.insert(
            "/telem_node/loop".to_string(),
            PathContract {
                input: vec![],
                output: vec!["/telem_node/status".into()],
                trigger: Some(EffectiveTrigger::Timer { rate_hz: 10.0 }),
                max_latency_ms: Some(100.0),
                ..Default::default()
            },
        );
        let mut pub_endpoints = std::collections::BTreeMap::new();
        pub_endpoints.insert("/control_node/cmd".to_string(), PubContract::default());
        pub_endpoints.insert("/telem_node/status".to_string(), PubContract::default());
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

    /// Issue 1285 follow-up. The target is a tier KEY, matched exactly. A board
    /// with no RTOS (`""`) and anything that is not a tier key derive no tier,
    /// and each placed node is recorded as a `tier` degradation. The old
    /// substring match wrote the POSIX sub-table for `""` and read
    /// `mps2-an385-freertos` as FreeRTOS.
    #[test]
    fn a_target_with_no_rtos_derives_no_tier_and_says_why() {
        let model = contract_model();
        let mut groups: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::new();
        groups.insert("control_node".into(), vec![cbg("ctrl", "high")]);
        groups.insert("telem_node".into(), vec![cbg("telem", "low")]);

        for target in ["", "mps2-an385-freertos", "zephyr-zenoh"] {
            let derived = derive_tiers_from_contracts(&model, target, &groups);
            assert!(derived.tiers.is_empty(), "{target:?}: {:?}", derived.tiers);
            assert!(derived.overrides.is_empty(), "{target:?}");
            let notes = derived
                .degradations
                .iter()
                .filter(|d| d.dim == "tier")
                .count();
            assert_eq!(notes, 2, "{target:?}: {:?}", derived.degradations);
        }

        // A real key still lands in its own sub-table, and only there.
        let z = derive_tiers_from_contracts(&model, "zephyr", &groups);
        let ctrl = &z.tiers["derived-control_node"];
        assert!(ctrl.zephyr.is_some() && ctrl.posix.is_none() && ctrl.freertos.is_none());

        // Resolving that table for a board with NO RTOS is refused BY NAME.
        // Not `MissingRtosSpec`, whose message would print `[tiers.x.]` and
        // advise adding a sub-table that does not exist. A real key with no
        // sub-table keeps the old error.
        let component_names: BTreeSet<&str> = ["control_node", "telem_node"].into_iter().collect();
        let no_rtos = resolve_tiers(&z.tiers, &z.overrides, &component_names, &groups, "")
            .expect_err("an authored tier on a no-RTOS board");
        assert!(
            matches!(no_rtos, crate::TierResolveError::NoRtosFamily { .. }),
            "{no_rtos:?}"
        );
        let missing = resolve_tiers(&z.tiers, &z.overrides, &component_names, &groups, "posix")
            .expect_err("a zephyr-only tier resolved for posix");
        assert!(
            matches!(missing, crate::TierResolveError::MissingRtosSpec { .. }),
            "{missing:?}"
        );
    }

    fn zephyr_priorities(d: &DerivedSchedule) -> BTreeMap<String, i64> {
        d.tiers
            .iter()
            .map(|(n, t)| {
                let p = t.zephyr.as_ref().map(|z| z.priority);
                (
                    n.clone(),
                    p.expect("a derived zephyr tier carries a priority"),
                )
            })
            .collect()
    }

    /// Issue 1508. The two image shapes the Kconfig DEFAULTS projection gets
    /// wrong: against the projection, rank 0 is 5, which is ABOVE a transport
    /// at k_thread 7 and INSIDE one at `[4, 9]`. Allocated out of the image's
    /// own plan, every derived tier lands in that image's `pool.app`, strictly
    /// below (numerically above) its transport.
    #[test]
    fn a_derived_table_is_allocated_out_of_the_images_own_plan() {
        let model = contract_model();
        let mut groups: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::new();
        groups.insert("control_node".into(), vec![cbg("ctrl", "high")]);
        groups.insert("telem_node".into(), vec![cbg("telem", "low")]);

        // The measured defect: the projection's numbers, whatever the image.
        let projected = zephyr_priorities(&derive_tiers_from_contracts(&model, "zephyr", &groups));
        assert_eq!(projected["derived-control_node"], 5, "{projected:?}");
        assert_eq!(projected["derived-telem_node"], 6, "{projected:?}");

        let gates = "CONFIG_NUM_COOP_PRIORITIES=16\nCONFIG_POSIX_PRIORITY_SCHEDULING=y\n\
                     CONFIG_PREEMPT_ENABLED=y\n";
        for (label, preempt, bands, want) in [
            ("NUM_PREEMPT_PRIORITIES=32", 32, [200i64, 200], [8i64, 9]),
            ("ZENOH_READ_PRIORITY=100", 15, [100, 200], [10, 11]),
        ] {
            let cfg = format!("CONFIG_NUM_PREEMPT_PRIORITIES={preempt}\n{gates}");
            let plan = PriorityPlan::from_zephyr_dotconfig(&cfg, &bands).expect("resolves");
            let transport = plan.reserved["transport"];
            // The projection's 5 is as urgent as or more urgent than this
            // image's least urgent transport thread: the inversion.
            assert!(
                projected["derived-control_node"] <= transport.hi,
                "{label}: the shape no longer reproduces issue 1508"
            );
            let got = zephyr_priorities(&derive_tiers_in_plan(&model, "zephyr", &groups, &plan));
            assert_eq!(
                [got["derived-control_node"], got["derived-telem_node"]],
                want,
                "{label}: transport {transport:?} pool {:?}",
                plan.app
            );
            for (n, p) in &got {
                assert!(
                    plan.app.contains(*p) && *p > transport.hi,
                    "{label}: {n} at {p}"
                );
            }
        }
    }

    /// Issue 1508, negative direction: on an image with Zephyr's default 15
    /// preemptive priorities and the default transport band, the image's own
    /// plan derives EXACTLY what the projection does (issue 1427's 5 / 6).
    #[test]
    fn the_default_image_derives_what_the_projection_does() {
        let model = contract_model();
        let mut groups: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::new();
        groups.insert("control_node".into(), vec![cbg("ctrl", "high")]);
        groups.insert("telem_node".into(), vec![cbg("telem", "low")]);
        let cfg = "CONFIG_NUM_PREEMPT_PRIORITIES=15\nCONFIG_NUM_COOP_PRIORITIES=16\n\
                   CONFIG_POSIX_PRIORITY_SCHEDULING=y\nCONFIG_PREEMPT_ENABLED=y\n";
        let plan = PriorityPlan::from_zephyr_dotconfig(cfg, &[200, 200]).expect("resolves");
        let image = derive_tiers_in_plan(&model, "zephyr", &groups, &plan);
        let projected = derive_tiers_from_contracts(&model, "zephyr", &groups);
        assert_eq!(zephyr_priorities(&image), zephyr_priorities(&projected));
        assert_eq!(
            format!("{:?}", image.tiers),
            format!("{:?}", projected.tiers)
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

    // ---- RFC-0079 section 6: a supervised deadline is allocated above the
    // transport band ----

    fn supervise(model: &mut SystemModel, path: &str) {
        model
            .contracts
            .node_paths
            .get_mut(path)
            .expect("path")
            .supervise = Some(ros_launch_manifest_model::SuperviseContract {
            route_deadline_ms: 5.0,
            reaction: "x".into(),
            mechanism: ros_launch_manifest_model::SuperviseMechanism::Timer,
        });
    }

    /// The island's image: transport [4, 4], pool.app [5, 14].
    fn island_plan() -> PriorityPlan {
        crate::priority_plan::zephyr_plan(15, 16, &[200, 200], "test")
    }

    fn prio(d: &DerivedSchedule, node: &str) -> i64 {
        d.realized
            .iter()
            .find(|n| n.name == node)
            .expect(node)
            .priority
    }

    fn groups() -> BTreeMap<String, Vec<CallbackGroupDecl>> {
        let mut g = BTreeMap::new();
        g.insert(
            "control_node".into(),
            vec![cbg("main", crate::DEFAULT_TIER)],
        );
        g.insert("telem_node".into(), vec![cbg("main", crate::DEFAULT_TIER)]);
        g
    }

    /// No `supervise:`: both nodes stay in pool.app, below the transport.
    #[test]
    fn without_supervision_every_rank_stays_below_the_transport() {
        let d = derive_tiers_in_plan(&contract_model(), "zephyr", &groups(), &island_plan());
        assert_eq!(prio(&d, "/control_node"), 5);
        assert_eq!(prio(&d, "/telem_node"), 6);
        assert!(d.above_transport.is_empty());
        assert_eq!(d.supervisor_priority, None);
    }

    fn set(names: &[&str]) -> std::collections::BTreeSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    /// Issue 1967: a supervised node whose tier does not name the transport
    /// stays where its rank puts it, below the transport, and that is said
    /// in a note; the supervisor still gets the priority adjacent to the
    /// transport, above it.
    #[test]
    fn a_supervised_node_stays_below_the_transport_unless_its_tier_names_it() {
        let mut m = contract_model();
        supervise(&mut m, "/telem_node/loop");
        let d = derive_tiers_in_plan(&m, "zephyr", &groups(), &island_plan());
        assert_eq!(prio(&d, "/telem_node"), 6, "pool.app, not moved");
        assert_eq!(prio(&d, "/control_node"), 5);
        assert!(d.above_transport.is_empty());
        assert_eq!(
            d.supervisor_priority,
            Some(3),
            "adjacent to the transport [4, 4]"
        );
        assert!(
            d.degradations
                .iter()
                .any(|x| x.node == "/telem_node" && x.reason.starts_with("note:")),
            "{:?}",
            d.degradations
        );
    }

    /// The island's shape: one allocated tier, three members, the most
    /// urgent supervised. Without the key the tier runs at its leader's
    /// pool rank (5) and the supervisor above the transport (3); with
    /// `above = "transport"` the leader's rank alone is lifted (3) and the
    /// supervisor goes one beyond it (2), the numbers #2312's implicit lift
    /// gave.
    #[test]
    fn a_tier_that_names_the_transport_is_lifted_with_its_leader_only() {
        let mut m = contract_model();
        supervise(&mut m, "/control_node/loop");
        m.execution.tiers.insert("mrm".into(), Default::default());
        let mut g = BTreeMap::new();
        g.insert("control_node".into(), vec![cbg("main", "mrm")]);
        g.insert("telem_node".into(), vec![cbg("main", "mrm")]);

        let below = derive_tiers_in_plan(&m, "zephyr", &g, &island_plan());
        assert_eq!(below.placements["mrm"].priority, 5);
        assert_eq!(below.supervisor_priority, Some(3));
        assert!(below.above_transport.is_empty());

        let above = derive_tiers_with_named(&m, "zephyr", &g, &island_plan(), &set(&["mrm"]));
        assert_eq!(above.placements["mrm"].priority, 3);
        assert_eq!(above.above_transport, vec!["/control_node".to_string()]);
        assert_eq!(prio(&above, "/telem_node"), 6, "not the leader: not lifted");
        assert_eq!(above.supervisor_priority, Some(2));
        assert!(
            !above
                .degradations
                .iter()
                .any(|x| x.reason.starts_with("note:")),
            "{:?}",
            above.degradations
        );
    }

    /// The island's shape: three nodes at three distinct ranks (control
    /// 100 Hz, telem 10 Hz, slow 1 Hz), each with a stated deadline.
    fn three_rank_model() -> SystemModel {
        let mut m = contract_model();
        m.structure.nodes.insert(
            "/slow_node".to_string(),
            NodeInstance {
                scope: "/".into(),
                ..Default::default()
            },
        );
        m.structure.topics.insert(
            "/slow".to_string(),
            TopicWiring {
                msg_type: "std_msgs/Int32".into(),
                publishers: vec!["/slow_node/out".into()],
                subscribers: vec![],
            },
        );
        m.contracts.node_paths.insert(
            "/slow_node/loop".to_string(),
            PathContract {
                input: vec![],
                output: vec!["/slow_node/out".into()],
                trigger: Some(EffectiveTrigger::Timer { rate_hz: 1.0 }),
                max_latency_ms: Some(1000.0),
                ..Default::default()
            },
        );
        m.contracts
            .pub_endpoints
            .insert("/slow_node/out".to_string(), PubContract::default());
        m
    }

    /// phase-509 P5 (RFC-0108 D4) - with no tier authored, the three ranks
    /// are three levels, each a tier of its own; under a cap of 2 where every
    /// band holds a deadline, the least urgent pair is merged anyway, named,
    /// and run as one `derived-level-<k>` tier.
    #[test]
    fn unbound_nodes_are_banded_into_levels_under_the_images_cap() {
        let m = three_rank_model();
        let mut g = BTreeMap::new();
        for node in ["control_node", "telem_node", "slow_node"] {
            g.insert(node.to_string(), vec![cbg("main", "default")]);
        }
        let d = derive_tiers_in_plan(&m, "zephyr", &g, &island_plan());
        let levels = d.levels.as_ref().expect("levels derived");
        assert_eq!(levels.levels.len(), 3);
        assert_eq!((levels.max_levels, levels.cap_source), (8, "platform"));
        assert_eq!(
            d.tiers.keys().collect::<Vec<_>>(),
            [
                "derived-control_node",
                "derived-slow_node",
                "derived-telem_node"
            ]
        );

        let mut capped = island_plan();
        capped.max_levels = Some(2);
        let d = derive_tiers_in_plan(&m, "zephyr", &g, &capped);
        let levels = d.levels.as_ref().expect("levels derived");
        assert_eq!(levels.method, crate::levels::LevelMethod::Capped);
        assert_eq!((levels.max_levels, levels.cap_source), (2, "image"));
        assert_eq!(
            d.tiers.keys().collect::<Vec<_>>(),
            ["derived-control_node", "derived-level-1"]
        );
        let shared = &d.tiers["derived-level-1"];
        assert_eq!(
            shared.zephyr.as_ref().map(|z| z.priority),
            Some(prio(&d, "/telem_node")),
            "the level runs at its most urgent member's priority"
        );
        assert_eq!(shared.deadline_us, Some(100_000), "the tighter deadline");
        let bound: Vec<(&str, &str)> = d
            .overrides
            .iter()
            .map(|o| (o.name.as_str(), o.callback_groups[0].tier.as_str()))
            .collect();
        assert!(
            bound.contains(&("slow_node", "derived-level-1")),
            "{bound:?}"
        );
        assert!(
            bound.contains(&("telem_node", "derived-level-1")),
            "{bound:?}"
        );
        let budget: Vec<&str> = d
            .degradations
            .iter()
            .filter(|x| x.dim == "level-budget")
            .map(|x| x.node.as_str())
            .collect();
        // The forced merge names the deadline it adds blocking to, and the
        // member that lost its rank is named too.
        assert_eq!(
            budget,
            vec!["/telem_node", "/slow_node"],
            "{:?}",
            d.degradations
        );
    }

    /// phase-509 P1 (RFC-0108 D4 rule 5). The island's shape: one authored,
    /// unplaced tier whose three members hold three distinct ranks (on the
    /// island 5, 7 and 8). The tier keeps the leader's rank, so BOTH other
    /// members lose theirs, and each is named in its own degradation. Before
    /// P1 only the first was named (`members.iter().find(...)`).
    #[test]
    fn every_member_that_loses_its_rank_is_reported() {
        let mut m = three_rank_model();
        m.execution.tiers.insert("mrm".into(), Default::default());
        let mut g = BTreeMap::new();
        for node in ["control_node", "telem_node", "slow_node"] {
            // Two groups on one node bound to the tier are still one member.
            g.insert(
                node.to_string(),
                vec![cbg("main", "mrm"), cbg("second", "mrm")],
            );
        }

        let d = derive_tiers_in_plan(&m, "zephyr", &g, &island_plan());
        let ranks: Vec<i64> = ["/control_node", "/telem_node", "/slow_node"]
            .iter()
            .map(|n| prio(&d, n))
            .collect();
        assert!(
            ranks[0] < ranks[1] && ranks[1] < ranks[2],
            "three distinct ranks, most urgent first: {ranks:?}"
        );
        assert_eq!(d.placements["mrm"].priority, ranks[0]);
        // phase-509 P5 - the authored tier is a pin: one level, all three
        // members, and nothing derived beside it.
        let levels = d.levels.as_ref().expect("levels derived");
        assert_eq!(levels.levels.len(), 1);
        assert_eq!(levels.levels[0].tiers, vec!["mrm".to_string()]);
        assert_eq!(levels.levels[0].members.len(), 3);
        assert!(d.tiers.is_empty());
        let lost: Vec<&str> = d
            .degradations
            .iter()
            .filter(|x| x.dim == "tier" && x.reason.contains("do not share a rank"))
            .map(|x| x.node.as_str())
            .collect();
        assert_eq!(
            lost,
            vec!["/telem_node", "/slow_node"],
            "{:?}",
            d.degradations
        );
        for (node, rank) in [("/telem_node", ranks[1]), ("/slow_node", ranks[2])] {
            let x = d
                .degradations
                .iter()
                .find(|x| x.node == node)
                .expect("reported");
            assert!(
                x.reason.contains(&format!("this node {rank}")),
                "{node}: {}",
                x.reason
            );
        }
    }

    /// A tier may name the transport with nothing supervised: it is lifted,
    /// and no supervisor priority is reserved.
    #[test]
    fn a_named_tier_without_supervision_is_lifted_and_reserves_no_supervisor() {
        let m = {
            let mut m = contract_model();
            m.execution.tiers.insert("fast".into(), Default::default());
            m
        };
        let mut g = groups();
        g.insert("telem_node".into(), vec![cbg("main", "fast")]);
        let d = derive_tiers_with_named(&m, "zephyr", &g, &island_plan(), &set(&["fast"]));
        assert_eq!(d.placements["fast"].priority, 3);
        assert_eq!(prio(&d, "/control_node"), 5);
        assert_eq!(d.supervisor_priority, None);
    }

    /// A plan without a transport band: the named tier stays in pool.app,
    /// and a degradation names the tier and the target.
    #[test]
    fn naming_the_transport_on_a_plan_without_one_degrades() {
        let mut m = contract_model();
        m.execution.tiers.insert("fast".into(), Default::default());
        let mut g = groups();
        g.insert("telem_node".into(), vec![cbg("main", "fast")]);
        let mut plan = island_plan();
        plan.reserved.remove("transport");
        let d = derive_tiers_with_named(&m, "zephyr", &g, &plan, &set(&["fast"]));
        assert!(d.above_transport.is_empty());
        assert!(
            d.degradations
                .iter()
                .any(|x| x.node == "[tiers.fast]" && x.reason.contains("no transport band")),
            "{:?}",
            d.degradations
        );
    }

    /// Two lifted ranks keep their order above the transport.
    #[test]
    fn lifted_nodes_keep_their_order_above_the_transport() {
        let mut nodes = vec![realized("/a", 5), realized("/b", 6), realized("/c", 7)];
        let mut deg = Vec::new();
        let (lifted, s) = lift_above_transport(
            &mut nodes,
            &set(&["/a", "/b"]),
            &set(&["/a"]),
            &island_plan(),
            true,
            &mut deg,
        );
        assert_eq!(
            (nodes[0].priority, nodes[1].priority, nodes[2].priority),
            (2, 3, 7)
        );
        assert_eq!(lifted.len(), 2);
        assert_eq!(s, Some(1));
        assert!(deg.is_empty(), "{deg:?}");
    }

    /// No room above the transport: nothing crosses, and it is said; the
    /// supervisor has no priority either.
    #[test]
    fn with_no_priority_above_the_transport_nothing_crosses() {
        // Band 255 resolves the transport to k_thread 0: nothing above it but
        // the cooperative priorities, which a derived placement never takes.
        let plan = crate::priority_plan::zephyr_plan(15, 16, &[255, 200], "test");
        let mut nodes = vec![realized("/a", 6)];
        let mut deg = Vec::new();
        let (lifted, s) = lift_above_transport(
            &mut nodes,
            &set(&["/a"]),
            &set(&["/a"]),
            &plan,
            true,
            &mut deg,
        );
        assert!(lifted.is_empty());
        assert_eq!(nodes[0].priority, 6, "stays in pool.app");
        assert_eq!(s, None);
        assert!(
            deg.iter().any(|x| x.reason.contains("kernel has 0")),
            "{deg:?}"
        );
        assert!(
            deg.iter().any(|x| x.reason.contains("no priority remains")),
            "{deg:?}"
        );
    }

    /// One priority above the transport: the node crosses, the supervisor
    /// has none left, and that is said.
    #[test]
    fn a_band_one_wide_lifts_the_node_and_reports_no_room_for_the_supervisor() {
        let mut nodes = vec![realized("/a", 6)];
        let mut plan = island_plan();
        plan.range = crate::priority_plan::Band::new(3, 14);
        let mut deg = Vec::new();
        let (lifted, s) = lift_above_transport(
            &mut nodes,
            &set(&["/a"]),
            &set(&["/a"]),
            &plan,
            true,
            &mut deg,
        );
        assert_eq!(nodes[0].priority, 3);
        assert_eq!(lifted, vec!["/a".to_string()]);
        assert_eq!(s, None);
        assert_eq!(deg.len(), 1);
    }

    /// A bigger-is-urgent kernel (FreeRTOS's shape): above is numerically up.
    #[test]
    fn on_a_bigger_is_urgent_kernel_above_is_numerically_higher() {
        let mut reserved = BTreeMap::new();
        reserved.insert(
            "transport".to_string(),
            crate::priority_plan::Band::new(4, 4),
        );
        let plan = PriorityPlan {
            direction: crate::priority_plan::Direction::BiggerIsUrgent,
            range: crate::priority_plan::Band::new(0, 7),
            reserved,
            app: crate::priority_plan::Band::new(1, 3),
            source: "test".into(),
            max_levels: None,
        };
        let mut nodes = vec![realized("/a", 3), realized("/b", 2), realized("/c", 1)];
        let mut deg = Vec::new();
        let (_, s) = lift_above_transport(
            &mut nodes,
            &set(&["/a", "/b"]),
            &set(&["/a"]),
            &plan,
            false,
            &mut deg,
        );
        assert_eq!(
            (nodes[0].priority, nodes[1].priority, nodes[2].priority),
            (6, 5, 1)
        );
        assert_eq!(s, Some(7));
        assert!(deg.is_empty());
        // Nothing lifted, one supervised: the supervisor is adjacent (5).
        let mut nodes = vec![realized("/a", 3)];
        let (_, s) =
            lift_above_transport(&mut nodes, &set(&[]), &set(&["/a"]), &plan, false, &mut deg);
        assert_eq!((nodes[0].priority, s), (3, Some(5)));
    }

    fn realized(name: &str, priority: i64) -> crate::rtos_realizer::RealizedNode {
        use crate::rtos_realizer::DimRealization::NotRequested;
        crate::rtos_realizer::RealizedNode {
            name: name.into(),
            priority,
            sched_class: "fifo",
            period_us: None,
            deadline_us: None,
            budget_us: None,
            core: None,
            preempt_threshold: None,
            blocking_us: None,
            deadline_real: NotRequested,
            budget_real: NotRequested,
            preempt_real: NotRequested,
            placement_real: NotRequested,
        }
    }
}
