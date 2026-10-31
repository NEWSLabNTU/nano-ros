//! Issue 1676 — the contract-monitor rows a resolved `SystemModel` bakes, in
//! ONE place both producers read.
//!
//! RFC-0052 / phase-296 W3b — a contracted publisher (`min_rate_hz`, a node
//! path's `max_latency_ms`) and a contracted subscriber (`max_age_ms`) each
//! become one row of the executor's monitor tables. The CLI's C and C++ entry
//! emitters bake these rows; the `nros::main!` proc-macro bakes the same rows
//! into the Rust entry. They lived in `nros-cli-core`, which the proc-macro
//! cannot depend on, so the Rust road had no rows at all and its
//! `system_monitors.rs` was baked and never installed. Here, the two roads
//! cannot derive two different tables.

use ros_launch_manifest_model::SystemModel;

/// A model a contract row cannot be derived from — a contracted endpoint the
/// wiring does not place on any topic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ContractRowError(pub String);

/// R1-N1 — one contracted-publisher monitor row extracted from the model
/// (RFC-0052 W3b.4 consumer side).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MonitorRow {
    /// Topic FQN (the wiring name the publisher creates).
    pub topic: String,
    /// Endpoint ref (`<node FQN>/<endpoint>`), the violation report key.
    pub fqn: String,
    /// Declared publisher guarantee, milli-Hz. 0 = no rate contract
    /// (latency-only row).
    pub min_rate_hz_milli: u32,
    /// W3b.5 — node-path budget (ms) for paths whose output is this
    /// endpoint (`contracts.node_paths`). 0 = no latency contract.
    #[serde(skip_serializing_if = "is_zero", default)]
    pub max_latency_ms: u32,
    /// phase10-B3 -- the wiring topic of the INPUT that triggers the
    /// tightest input-triggered node path whose output is this endpoint
    /// (its first input), or empty for none.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub route_trigger: String,
    /// phase10-B3 -- that path's `max_latency_ms`, judged from the take of
    /// the trigger to this output (`route-latency-runtime`). 0 = none.
    #[serde(skip_serializing_if = "is_zero", default)]
    pub route_max_latency_ms: u32,
    /// phase-503 -- the route's SUPERVISED deadline (ms) from the take of the
    /// trigger (`route-deadline-runtime`: the executor reports a take still
    /// unanswered at `take + route_deadline_ms` and calls its reaction hook).
    /// 0 = not supervised, the default: a model that states no supervision
    /// bakes exactly the row it baked before.
    #[serde(skip_serializing_if = "is_zero", default)]
    pub route_deadline_ms: u32,
    /// phase-503 -- the reaction the deadline starts: the contract's path
    /// ref, or empty for none.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub reaction: String,
    /// phase-503 -- the id the reaction hook receives for [`Self::reaction`]:
    /// 1-based over the model's distinct reaction refs in sorted order
    /// ([`reaction_ids`]); 0 = none.
    #[serde(skip_serializing_if = "is_zero_u16", default)]
    pub reaction_id: u16,
    /// phase-503 -- [`MECHANISM_EXECUTOR`] or [`MECHANISM_TIMER`].
    #[serde(skip_serializing_if = "is_zero_u8", default)]
    pub mechanism: u8,
}

/// phase-503 -- [`MonitorRow::mechanism`]: checked in the executor's spin loop.
pub const MECHANISM_EXECUTOR: u8 = 0;
/// phase-503 -- [`MonitorRow::mechanism`]: an independent timer outside the
/// executor.
pub const MECHANISM_TIMER: u8 = 1;

/// phase-503 -- the mechanism a contract names (`executor` | `timer`), as the
/// row's code. Anything else is refused by the caller.
#[must_use]
pub fn mechanism_code(name: &str) -> Option<u8> {
    match name {
        "executor" => Some(MECHANISM_EXECUTOR),
        "timer" => Some(MECHANISM_TIMER),
        _ => None,
    }
}

/// phase-503 -- the route supervision one output carries: its deadline (ms),
/// the reaction path ref, and the mechanism code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteSupervision {
    pub deadline_ms: u32,
    pub reaction: String,
    pub mechanism: u8,
}

/// phase-503 -- the reaction table: every distinct reaction ref the rows name,
/// sorted, with the id the hook receives for it (1-based). The generated
/// entries print it beside the rows, so a hook can map an id back to the
/// path the contract named.
#[must_use]
pub fn reaction_ids(rows: &[MonitorRow]) -> Vec<(u16, String)> {
    let mut refs: Vec<&str> = rows
        .iter()
        .filter(|r| !r.reaction.is_empty())
        .map(|r| r.reaction.as_str())
        .collect();
    refs.sort_unstable();
    refs.dedup();
    refs.into_iter()
        .enumerate()
        .map(|(i, r)| ((i + 1).min(u16::MAX as usize) as u16, r.to_string()))
        .collect()
}

/// phase-462 W3c -- the reaction table for a whole MODEL: every route's
/// supervised `reaction` and every hazard reaction path that states a
/// `safe_state`, sorted, numbered as [`reaction_ids`] numbers them.
///
/// One id space for routes and hazards, and one per model rather than per
/// executor table, so the id a hook receives names the same path whichever
/// executor reports it and whichever rule (`route-deadline-runtime` or
/// `hazard-detected`) asks for it. A model with no hazard numbers exactly
/// the refs it numbered before: the supervised rows' reactions.
#[must_use]
pub fn model_reaction_ids(model: &SystemModel) -> Vec<(u16, String)> {
    let mut refs: Vec<&str> = model
        .contracts
        .node_paths
        .values()
        .filter_map(|p| p.supervise.as_ref())
        .map(|s| s.reaction.as_str())
        .filter(|r| !r.is_empty())
        .collect();
    for (key, h) in &model.contracts.hazards {
        if let Ok(Some(r)) = crate::ftti_budget::reaction_path(model, key, h)
            && reaction_safe_state(model, &r).is_some()
        {
            refs.push(r.key);
        }
    }
    refs.sort_unstable();
    refs.dedup();
    refs.into_iter()
        .enumerate()
        .map(|(i, r)| ((i + 1).min(u16::MAX as usize) as u16, r.to_string()))
        .collect()
}

/// phase-503 -- number every row's reaction by [`reaction_ids`].
pub fn assign_reaction_ids(rows: &mut [MonitorRow]) {
    let ids = reaction_ids(rows);
    for r in rows.iter_mut() {
        r.reaction_id = ids
            .iter()
            .find(|(_, name)| *name == r.reaction)
            .map_or(0, |(id, _)| *id);
    }
}

/// phase-503 W5 -- the supervision a node path's `max_latency` states
/// (`contracts.node_paths.<key>.supervise`, play_launch phase 93), as the row
/// carries it. `None` when the path states none. A deadline that rounds to 0 ms
/// is refused: 0 is the row's "not supervised", so it would silently drop the
/// supervision the contract asked for.
fn route_supervision_of(
    path_ref: &str,
    p: &ros_launch_manifest_model::PathContract,
) -> Result<Option<RouteSupervision>, ContractRowError> {
    use ros_launch_manifest_model::SuperviseMechanism;
    let Some(s) = &p.supervise else {
        return Ok(None);
    };
    let deadline_ms = s.route_deadline_ms.round().max(0.0).min(u32::MAX as f64) as u32;
    if deadline_ms == 0 {
        return Err(ContractRowError(format!(
            "SystemModel: node path '{path_ref}' is supervised with a route_deadline_ms of \
             {} -- a deadline under 0.5 ms cannot be supervised",
            s.route_deadline_ms
        )));
    }
    Ok(Some(RouteSupervision {
        deadline_ms,
        reaction: s.reaction.clone(),
        mechanism: match s.mechanism {
            SuperviseMechanism::Executor => MECHANISM_EXECUTOR,
            SuperviseMechanism::Timer => MECHANISM_TIMER,
        },
    }))
}

/// phase-503 -- attach supervision to the rows by output endpoint ref. A row
/// whose route is not stated in `sup` is left unsupervised; a supervision for
/// an output with no route row is refused, since nothing would watch it.
/// Reaction ids are (re)assigned over the whole table.
///
/// [`monitor_rows`] feeds it from the model's `supervise:` on each
/// input-triggered node path (phase-503 W5).
pub fn apply_route_supervision(
    rows: &mut [MonitorRow],
    sup: &std::collections::BTreeMap<String, RouteSupervision>,
) -> Result<(), ContractRowError> {
    for (out, s) in sup {
        let Some(row) = rows
            .iter_mut()
            .find(|r| &r.fqn == out && r.route_max_latency_ms > 0)
        else {
            return Err(ContractRowError(format!(
                "SystemModel: supervised route output '{out}' has no input-triggered \
                 node path with max_latency_ms -- nothing to supervise"
            )));
        };
        row.route_deadline_ms = s.deadline_ms;
        row.reaction = s.reaction.clone();
        row.mechanism = s.mechanism;
    }
    assign_reaction_ids(rows);
    Ok(())
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

fn is_zero_u16(v: &u16) -> bool {
    *v == 0
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

/// W3b.5 — one contracted-subscriber age row (`sub_endpoints.max_age_ms`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AgeRow {
    /// Topic FQN (the wiring name the subscriber creates).
    pub topic: String,
    /// Endpoint ref, the violation report key.
    pub fqn: String,
    /// Declared max take-age, ms. 0 = the row exists for its `deadline_ms`
    /// alone.
    pub max_age_ms: u32,
    /// phase-462 W5b -- the contract's `deadline` for this endpoint (its own
    /// `qos:` block, else the topic's), in the wire's milliseconds. 0 = none.
    /// The executor judges it only where the subscription's backend cannot.
    pub deadline_ms: u32,
}

/// Extract the publisher rate-monitor rows: every `pub_endpoints` entry
/// with `min_rate_hz`, joined to the topic whose wiring lists it as a
/// publisher. A contracted endpoint with NO owning topic in the wiring is
/// a model inconsistency — fail loud.
pub fn monitor_rows(model: &SystemModel) -> Result<Vec<MonitorRow>, ContractRowError> {
    use std::collections::BTreeMap;
    // fqn -> (min_rate_milli, max_latency_ms); node_paths may add
    // latency-only rows for endpoints without a rate contract.
    let mut by_fqn: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for (ep_ref, c) in &model.contracts.pub_endpoints {
        let Some(min) = c.min_rate_hz else { continue };
        by_fqn.insert(
            ep_ref.clone(),
            (
                (min * 1000.0).round().max(0.0).min(u32::MAX as f64) as u32,
                0,
            ),
        );
    }
    // phase-462 W3b -- a hazard's reaction is observed when its `safe_state`
    // publisher publishes, so that publisher needs a counter cell even when it
    // promises no rate: a count-only row (no rate, no latency) judges nothing
    // and gives the observer the count it reads.
    for (sink, _) in hazard_sinks(model) {
        by_fqn.entry(sink).or_insert((0, 0));
    }
    // phase-82 follow-up: a path may name a service CLIENT in its output
    // (the reaction walk follows that edge across the service). A call is
    // not a publisher: it has no rate promise and no owning topic, so it
    // gets no monitor row rather than tripping the owning-topic check below.
    let service_clients: std::collections::BTreeSet<&String> = model
        .structure
        .services
        .values()
        .flat_map(|s| s.client.iter())
        .collect();
    // phase10-B3 -- output fqn -> (route budget, trigger topic): the
    // tightest input-triggered path that names the output.
    // phase-503 W5 -- plus that path's supervision, when it states one.
    let mut routes: BTreeMap<String, (u32, String, Option<RouteSupervision>)> = BTreeMap::new();
    // W3b.5 — node-path budgets attach to the path's OUTPUT endpoints.
    for (path_ref, p) in &model.contracts.node_paths {
        let Some(lat) = p.max_latency_ms else {
            continue;
        };
        let lat = lat.round().max(0.0).min(u32::MAX as f64) as u32;
        if lat == 0 {
            continue;
        }
        if p.output.is_empty() {
            return Err(ContractRowError(format!(
                "SystemModel: node path '{path_ref}' declares max_latency_ms but \
                 lists no output endpoint — inconsistent model"
            )));
        }
        // phase10-B3 -- an input-triggered path is a route from the take of
        // its input to its output. A path with several inputs is judged from
        // its first, the one the contract names first.
        let trigger_topic = p.input.first().and_then(|input| {
            model
                .structure
                .topics
                .iter()
                .find(|(_, w)| w.subscribers.iter().any(|s| s == input))
                .map(|(t, _)| t.clone())
        });
        // phase-503 W5 -- `supervise:` on the path's `max_latency` (phase 93):
        // the deadline, the reaction path id and the mechanism the route row
        // carries to the executor.
        let supervision = route_supervision_of(path_ref, p)?;
        for out in &p.output {
            if service_clients.contains(out) {
                continue;
            }
            let e = by_fqn.entry(out.clone()).or_insert((0, 0));
            e.1 = e.1.max(lat);
            if let Some(topic) = &trigger_topic {
                let r =
                    routes
                        .entry(out.clone())
                        .or_insert((lat, topic.clone(), supervision.clone()));
                if lat < r.0 {
                    *r = (lat, topic.clone(), supervision.clone());
                }
            } else if supervision.is_some() {
                return Err(ContractRowError(format!(
                    "SystemModel: node path '{path_ref}' is supervised but its trigger \
                     input is on no topic in structure.topics -- no route to supervise"
                )));
            }
        }
    }
    let mut rows = Vec::new();
    let mut sup: BTreeMap<String, RouteSupervision> = BTreeMap::new();
    for (ep_ref, (min_milli, lat_ms)) in by_fqn {
        let topic = model
            .structure
            .topics
            .iter()
            .find(|(_, w)| w.publishers.iter().any(|p| p == &ep_ref))
            .map(|(t, _)| t.clone());
        let Some(topic) = topic else {
            return Err(ContractRowError(format!(
                "SystemModel: contracted publisher '{ep_ref}' has no \
                 owning topic in structure.topics — inconsistent model"
            )));
        };
        let (route_max_latency_ms, route_trigger, supervision) =
            routes.remove(&ep_ref).unwrap_or_default();
        if let Some(s) = supervision {
            sup.insert(ep_ref.clone(), s);
        }
        rows.push(MonitorRow {
            topic,
            fqn: ep_ref,
            min_rate_hz_milli: min_milli,
            max_latency_ms: lat_ms,
            route_trigger,
            route_max_latency_ms,
            route_deadline_ms: 0,
            reaction: String::new(),
            reaction_id: 0,
            mechanism: 0,
        });
    }
    apply_route_supervision(&mut rows, &sup)?;
    // phase-462 W3c -- renumber in the MODEL's id space, which hazards share.
    let ids = model_reaction_ids(model);
    for r in rows.iter_mut() {
        r.reaction_id = ids
            .iter()
            .find(|(_, name)| *name == r.reaction)
            .map_or(0, |(id, _)| *id);
    }
    Ok(rows)
}

/// W3b.5 — extract the subscriber age rows: every `sub_endpoints` entry
/// with `max_age_ms`, joined to the topic whose wiring lists it as a
/// subscriber. Orphans fail loud (same rule as the publisher join).
///
/// phase-462 W5b -- and every one whose contract states a `deadline`, so a
/// backend that cannot judge it hands it to the silence rule rather than
/// refusing it at boot.
pub fn age_rows(model: &SystemModel) -> Result<Vec<AgeRow>, ContractRowError> {
    let mut rows = Vec::new();
    for (ep_ref, c) in &model.contracts.sub_endpoints {
        let deadline = contract_deadline_ms(model, ep_ref, c);
        if c.max_age_ms.is_none() && deadline == 0 {
            continue;
        }
        let what = if c.max_age_ms.is_some() {
            "max_age_ms"
        } else {
            "deadline"
        };
        let topic = model
            .structure
            .topics
            .iter()
            .find(|(_, w)| w.subscribers.iter().any(|p| p == ep_ref))
            .map(|(t, _)| t.clone());
        let Some(topic) = topic else {
            return Err(ContractRowError(format!(
                "SystemModel: contracted subscriber '{ep_ref}' ({what}) has no \
                 owning topic in structure.topics — inconsistent model"
            )));
        };
        rows.push(AgeRow {
            topic,
            fqn: ep_ref.clone(),
            max_age_ms: c
                .max_age_ms
                .map_or(0, |age| age.round().max(1.0).min(u32::MAX as f64) as u32),
            deadline_ms: deadline,
        });
    }
    rows.sort_by(|a, b| a.fqn.cmp(&b.fqn));
    Ok(rows)
}

/// phase-462 W5b -- a subscriber's contract `deadline` in the wire's
/// milliseconds: the endpoint's own `qos:` statement, else its topic's (the
/// same fact one level up, as `qos_agreement` reads it), converted by the
/// rule the runtime uses. 0 when neither states one.
fn contract_deadline_ms(
    model: &SystemModel,
    ep_ref: &str,
    c: &ros_launch_manifest_model::SubContract,
) -> u32 {
    let own = c.qos.as_ref().and_then(|q| q.deadline_ms);
    let topic = || {
        model
            .structure
            .topics
            .iter()
            .find(|(_, w)| w.subscribers.iter().any(|p| p == ep_ref))
            .and_then(|(t, _)| model.contracts.topics.get(t))
            .and_then(|t| t.qos.as_ref())
            .and_then(|q| q.deadline_ms)
    };
    own.or_else(topic)
        .and_then(crate::qos_override::contract_ms_to_wire_ms)
        .unwrap_or(0)
}

/// The safe state a hazard's reaction commands. A reaction that is a node
/// path states it there; a reaction that is a scope path -- the shape
/// play_launch resolves every contract to -- has none of its own: the safe
/// state is on the node path whose `emits` publishes one of the scope path's
/// outputs (the node at the reaction's sink). When several do, the one with
/// the largest settle, so the hazard is charged the slowest. Reading only
/// the reaction path's own left `settle_ms` 0 and the hazard with no sink
/// for every resolved model (play_launch phase 91 E10's test found it).
pub(crate) fn reaction_safe_state<'m>(
    model: &'m SystemModel,
    r: &crate::ftti_budget::ResolvedReaction<'m>,
) -> Option<&'m ros_launch_manifest_model::SafeStateContract> {
    if let Some(ss) = &r.path.safe_state {
        return Some(ss);
    }
    if !model.contracts.scope_paths.contains_key(r.key) {
        return None;
    }
    model
        .contracts
        .node_paths
        .values()
        .filter_map(|p| p.safe_state.as_ref())
        .filter(|ss| {
            r.path.output.iter().any(|t| {
                model
                    .structure
                    .topics
                    .get(t)
                    .is_some_and(|w| w.publishers.contains(&ss.emits))
            })
        })
        .max_by(|a, b| {
            a.settle_ms
                .unwrap_or(0.0)
                .total_cmp(&b.settle_ms.unwrap_or(0.0))
                .then_with(|| b.emits.cmp(&a.emits))
        })
}

/// phase-462 W3b -- every hazard's `safe_state` publisher in this model, with
/// its `settle_ms` (0 when none), keyed by endpoint ref; the safe state is
/// [`reaction_safe_state`]'s. A
/// hazard whose reaction does not resolve is skipped here: that refusal is
/// [`crate::ftti_budget`]'s, raised on both bake roads before anything bakes.
fn hazard_sinks(model: &SystemModel) -> std::collections::BTreeMap<String, u32> {
    let mut out = std::collections::BTreeMap::new();
    for (key, h) in &model.contracts.hazards {
        let Ok(Some(r)) = crate::ftti_budget::reaction_path(model, key, h) else {
            continue;
        };
        if let Some(ss) = reaction_safe_state(model, &r) {
            let settle = ss
                .settle_ms
                .unwrap_or(0.0)
                .round()
                .max(0.0)
                .min(u32::MAX as f64) as u32;
            out.insert(ss.emits.clone(), settle);
        }
    }
    out
}

/// phase-462 W3b -- one guard member, as a row of THIS table set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum GuardMemberRow {
    /// Index into the age rows.
    Age(usize),
    /// Index into the rate/latency rows.
    Rate(usize),
}

/// phase-462 W3b -- one guard of a [`HazardRow`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HazardGuardRow {
    /// Faults only when every member has.
    pub all_of: bool,
    /// The members, each a row of this table set.
    pub members: Vec<GuardMemberRow>,
}

/// phase-462 W3b -- one contracted hazard, lowered onto one executor's
/// monitor tables (the observer reads those rows' verdicts).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HazardRow {
    /// `contracts.hazards` key.
    pub key: String,
    /// `ftti_ms`, rounded; 0 = none declared.
    pub ftti_ms: u32,
    /// The reacting path's `safe_state.settle_ms`; 0 = none.
    pub settle_ms: u32,
    /// The guards, members resolved to rows.
    pub guards: Vec<HazardGuardRow>,
    /// Rate-row index of the `safe_state` publisher, when this table set has it.
    pub sink: Option<usize>,
    /// phase-462 W3c -- the id the reaction hook receives on `hazard-detected`
    /// ([`model_reaction_ids`]), or 0 when the reacting path states no
    /// `safe_state`: then there is no safe-state entry to start.
    pub reaction_id: u16,
    /// play_launch phase 96 W4.2 (phase 85 D4) -- the rung whose ROUTE this
    /// table set judges ([`hazard_rung`]), when there is one.
    pub rung: Option<HazardRungRow>,
}

/// play_launch phase 96 W4.2 -- the rung of a hazard's ladder an executor
/// judges, and the route play_launch's checker charged it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HazardRungRow {
    /// The rung's name (`contracts.hazards.<h>.rungs[].rung`), for comments.
    pub name: String,
    /// Rate-row index of a publisher on one of the rung's `output` topics.
    pub row: usize,
    /// The rung's `route_ms`, rounded UP, never 0: from the detection fire
    /// to the rung's first output.
    pub route_ms: u32,
}

/// play_launch phase 96 W4.2 -- the rung `rows` can judge: the TOP rung of the
/// ladder (the model lists rungs top first, and a rung the fault removes is
/// not listed) whose output a rate row of this table set publishes, provided
/// play_launch charged it a route.
///
/// Not a lower rung when the top observed one has no route: the top rung is
/// the one that answers first while it is available, so judging a lower rung
/// against the first output would compare the wrong quantity. Which rung
/// actually runs depends on the mode the image is in when the fault comes,
/// which this table does not track; the top rung is the claim the checker
/// makes for the healthy system.
pub fn hazard_rung(
    h: &ros_launch_manifest_model::HazardContract,
    rows: &[MonitorRow],
) -> Option<HazardRungRow> {
    let (rung, row) = h.rungs.iter().find_map(|r| {
        let row = rows.iter().position(|m| r.output.contains(&m.topic))?;
        Some((r, row))
    })?;
    let route = rung.route_ms.filter(|v| v.is_finite() && *v > 0.0)?;
    Some(HazardRungRow {
        name: rung.rung.clone(),
        row,
        route_ms: route.ceil().min(u32::MAX as f64).max(1.0) as u32,
    })
}

/// The rows of `rows`/`ages` that detect a fault on `topic`: every age row on
/// it (`silence-runtime`) and every rate row with a rate promise
/// (`min-rate-runtime`).
fn topic_members(topic: &str, rows: &[MonitorRow], ages: &[AgeRow]) -> Vec<GuardMemberRow> {
    let mut out: Vec<GuardMemberRow> = ages
        .iter()
        .enumerate()
        // A deadline-only row detects nothing the build can count on: it
        // judges only where the backend cannot, which is a run-time fact.
        .filter(|(_, a)| a.topic == topic && a.max_age_ms > 0)
        .map(|(i, _)| GuardMemberRow::Age(i))
        .collect();
    out.extend(
        rows.iter()
            .enumerate()
            .filter(|(_, r)| r.topic == topic && r.min_rate_hz_milli > 0)
            .map(|(i, _)| GuardMemberRow::Rate(i)),
    );
    out
}

/// The endpoints the MODEL contracts a detector on for `topic`, on whichever
/// board or tier: subscribers with `max_age_ms`, publishers with
/// `min_rate_hz`.
fn topic_detectors_in_model(model: &SystemModel, topic: &str) -> Vec<String> {
    let Some(w) = model.structure.topics.get(topic) else {
        return Vec::new();
    };
    let subs = w.subscribers.iter().filter(|s| {
        model
            .contracts
            .sub_endpoints
            .get(*s)
            .is_some_and(|c| c.max_age_ms.is_some())
    });
    let pubs = w.publishers.iter().filter(|p| {
        model
            .contracts
            .pub_endpoints
            .get(*p)
            .is_some_and(|c| c.min_rate_hz.is_some())
    });
    subs.chain(pubs).cloned().collect()
}

/// phase-462 W3b -- the hazards this table set observes, as rows indexing it.
///
/// `rows` / `ages` are the tables ONE executor installs (already sliced to the
/// nodes it runs). A hazard is lowered here only when every guard topic the
/// model monitors at all is monitored by THIS set, so one executor sees the
/// whole hazard and no hazard is observed twice by two executors that each see
/// part of it. A guard topic nothing monitors anywhere is left out (no
/// executor could see it, which `ftti_budget` reports as unobserved); for an
/// `all_of` guard that leaves the guard unable to fault, so it is dropped.
/// A hazard left with no guard is not lowered.
pub fn hazard_rows(model: &SystemModel, rows: &[MonitorRow], ages: &[AgeRow]) -> Vec<HazardRow> {
    let sinks = hazard_sinks(model);
    let ids = model_reaction_ids(model);
    let mut out = Vec::new();
    'hazard: for (key, h) in &model.contracts.hazards {
        let mut guards = Vec::new();
        for g in &h.guards {
            let mut members = Vec::new();
            let mut whole = true;
            for topic in &g.members {
                let in_model = topic_detectors_in_model(model, topic);
                let held = |fqn: &String| {
                    ages.iter().any(|a| &a.fqn == fqn) || rows.iter().any(|r| &r.fqn == fqn)
                };
                if !in_model.iter().all(held) {
                    // Monitored, at least in part, by another executor's table.
                    continue 'hazard;
                }
                let here = topic_members(topic, rows, ages);
                if here.is_empty() {
                    // Nothing anywhere watches this topic.
                    whole = false;
                    continue;
                }
                if g.all_of {
                    // One detector per member keeps "every member faulted"
                    // meaning every TOPIC; the age row is the faster one.
                    members.push(here[0]);
                } else {
                    members.extend(here);
                }
            }
            if members.is_empty() || (g.all_of && !whole) {
                continue;
            }
            guards.push(HazardGuardRow {
                all_of: g.all_of,
                members,
            });
        }
        if guards.is_empty() {
            continue;
        }
        let reaction = crate::ftti_budget::reaction_path(model, key, h)
            .ok()
            .flatten();
        let safe_state = reaction
            .as_ref()
            .and_then(|r| reaction_safe_state(model, r));
        let emits = safe_state.map(|s| s.emits.as_str());
        let reaction_id = reaction
            .filter(|_| safe_state.is_some())
            .and_then(|r| ids.iter().find(|(_, name)| name == r.key))
            .map_or(0, |(id, _)| *id);
        out.push(HazardRow {
            key: key.clone(),
            ftti_ms: h
                .ftti_ms
                .map_or(0, |v| v.round().max(0.0).min(u32::MAX as f64) as u32),
            settle_ms: emits.and_then(|e| sinks.get(e).copied()).unwrap_or(0),
            guards,
            sink: emits.and_then(|e| rows.iter().position(|r| r.fqn == e)),
            reaction_id,
            rung: hazard_rung(h, rows),
        });
    }
    out
}

/// phase-462 W3b -- the baked hazard table, as Rust: one `HazardCell` static
/// per row, the guard member slices, `NROS_HAZARDS`, and nothing when there
/// are no rows. Appended to the monitor include by
/// [`render_monitor_rs_full`].
fn render_hazards_rs(hazards: &[HazardRow], paths: RenderPaths) -> String {
    let mut out = String::new();
    if hazards.is_empty() {
        out.push_str(&format!(
            "pub static NROS_HAZARDS: &[{m}::HazardSpec] = &[];\n",
            m = paths.monitor
        ));
        return out;
    }
    out.push_str(&format!(
        "use {m}::{{GuardRef, GuardSpec, HazardCell, HazardSpec}};\n\n",
        m = paths.monitor
    ));
    for (i, h) in hazards.iter().enumerate() {
        out.push_str(&format!(
            "static NROS_HAZARD_CELL_{i}: HazardCell = HazardCell::new();\n"
        ));
        out.push_str(&format!(
            "const NROS_HAZARD_GUARDS_{i}: &[GuardSpec] = &[\n"
        ));
        for g in &h.guards {
            let members: Vec<String> = g
                .members
                .iter()
                .map(|m| match m {
                    GuardMemberRow::Age(r) => format!("GuardRef::Age({r}u16)"),
                    GuardMemberRow::Rate(r) => format!("GuardRef::Rate({r}u16)"),
                })
                .collect();
            out.push_str(&format!(
                "    GuardSpec {{ all_of: {a}, members: &[{m}] }},\n",
                a = g.all_of,
                m = members.join(", ")
            ));
        }
        out.push_str("];\n");
    }
    out.push_str("\npub static NROS_HAZARDS: &[HazardSpec] = &[\n");
    for (i, h) in hazards.iter().enumerate() {
        let sink = match h.sink {
            Some(r) => format!("{r}u16"),
            None => format!("{m}::NO_SINK", m = paths.monitor),
        };
        let (rung_row, rung_route) = match &h.rung {
            Some(r) => (format!("{}u16", r.row), r.route_ms),
            None => (format!("{m}::NO_SINK", m = paths.monitor), 0),
        };
        out.push_str(&format!(
            "    HazardSpec {{ key: {k:?}, ftti_ms: {f}u32, settle_ms: {st}u32, \
             guards: NROS_HAZARD_GUARDS_{i}, sink: {sink}, reaction_id: {ri}u16, \
             rung_row: {rung_row}, rung_route_ms: {rung_route}u32, \
             cell: &NROS_HAZARD_CELL_{i} }},\n",
            k = h.key,
            f = h.ftti_ms,
            st = h.settle_ms,
            ri = h.reaction_id,
        ));
    }
    out.push_str("];\n");
    out
}

/// R1-N1 — render the baked Rust monitor-table include
/// (`nros-system/system_monitors.rs`): one `PubMonitorCell` static per
/// contracted publisher + the `MONITORS` spec table + an installer the
/// generated entry calls before entity creation. Empty rows → an empty
/// table (DCE'd — the zero-cost gate).
pub fn render_monitor_rs(rows: &[MonitorRow], ages: &[AgeRow]) -> String {
    render_monitor_rs_at(rows, ages, RenderPaths::NROS_NODE)
}

/// [`render_monitor_rs`] with the type paths stated — issue 1676.
pub fn render_monitor_rs_at(rows: &[MonitorRow], ages: &[AgeRow], paths: RenderPaths) -> String {
    render_monitor_rs_full(rows, ages, &[], paths)
}

/// [`render_monitor_rs_at`] plus phase-462 W3b's hazard table: the installer
/// also calls `set_hazard_table(NROS_HAZARDS)`, after the two monitor tables
/// whose rows the hazards index.
pub fn render_monitor_rs_full(
    rows: &[MonitorRow],
    ages: &[AgeRow],
    hazards: &[HazardRow],
    paths: RenderPaths,
) -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED by `nros codegen-system --model` (RFC-0052 W3b.4/.5 / phase-296 N1).\n\
         // One PubMonitorCell per contracted publisher (+ one SubMonitorCell per age\n\
         // contract) + the executor monitor tables. Include from the entry; call\n\
         // `nros_install_monitors(&mut executor)` BEFORE entity creation (node-side\n\
         // attachment is auto-seeded from the executor at create_node).\n",
    );
    out.push_str(&format!(
        "use {m}::{{AgeMonitorSpec, MonitorSpec, PubMonitorCell, SubMonitorCell}};\n\n",
        m = paths.monitor
    ));
    for (i, _r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "static NROS_MONITOR_CELL_{i}: PubMonitorCell = PubMonitorCell::new();\n"
        ));
    }
    for (i, _r) in ages.iter().enumerate() {
        out.push_str(&format!(
            "static NROS_AGE_CELL_{i}: SubMonitorCell = SubMonitorCell::new();\n"
        ));
    }
    out.push('\n');
    out.push_str(&render_spec_tables(
        rows,
        ages,
        "NROS_MONITORS",
        "NROS_AGE_MONITORS",
    ));
    // phase-496 W1 (issue 1471) -- the executor watches at most
    // `MAX_MONITORS` / `MAX_AGE_MONITORS` rows, and the spin loop inspects only
    // that many. A table past either is a COMPILE error here that names the
    // knob, never a table that boots with its tail unwatched. Both knobs derive
    // from these same row counts, so this fires only when a stated value is
    // below the contract. An empty table cannot overflow and gets no assert
    // (`0 <= N` is clippy's deny-level `absurd_extreme_comparisons`).
    for (n, bound, knob, what) in [
        (
            rows.len(),
            "MAX_MONITORS",
            "NROS_EXECUTOR_MAX_MONITORS",
            "rate/latency",
        ),
        (
            ages.len(),
            "MAX_AGE_MONITORS",
            "NROS_EXECUTOR_MAX_AGE_MONITORS",
            "age",
        ),
    ] {
        if n == 0 {
            continue;
        }
        out.push_str(&format!(
            "const _: () = assert!(\n    {n} <= {m}::{bound},\n    \
             \"this contract bakes {n} {what} monitor rows, more than the executor watches: \
             raise {knob} (it derives from the contract when nothing states it)\"\n);\n",
            m = paths.monitor
        ));
    }
    out.push('\n');
    if hazards.is_empty() {
        out.push_str(
            &format!(
                "pub fn nros_install_monitors(executor: &mut {e}<'_>) {{\n    executor.set_monitor_table(NROS_MONITORS);\n    executor.set_age_table(NROS_AGE_MONITORS);\n}}\n",
                e = paths.executor
            ),
        );
        return out;
    }
    out.push_str(&render_hazards_rs(hazards, paths));
    out.push('\n');
    out.push_str(
        &format!(
            "pub fn nros_install_monitors(executor: &mut {e}<'_>) {{\n    executor.set_monitor_table(NROS_MONITORS);\n    executor.set_age_table(NROS_AGE_MONITORS);\n    executor.set_hazard_table(NROS_HAZARDS);\n}}\n",
            e = paths.executor
        ),
    );
    out
}

/// The two `pub static` spec tables over `rows` / `ages`, naming the cells
/// [`render_monitor_rs_full`] declares (`NROS_MONITOR_CELL_<i>` /
/// `NROS_AGE_CELL_<i>`, by row index). One spelling, so a phase-462 W4 mode
/// variant ([`crate::mode_variants`]) is the base's text with its numbers.
pub(crate) fn render_spec_tables(
    rows: &[MonitorRow],
    ages: &[AgeRow],
    monitors_name: &str,
    ages_name: &str,
) -> String {
    let mut out = format!("pub static {monitors_name}: &[MonitorSpec] = &[\n");
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "    MonitorSpec {{ topic: {t:?}, fqn: {f:?}, min_rate_hz_milli: {m}u32, \
             max_latency_ms: {l}u32, route_trigger: {rt:?}, route_max_latency_ms: {rl}u32, \
             route_deadline_ms: {rd}u32, reaction_id: {ri}u16, mechanism: {mc}u8, \
             cell: &NROS_MONITOR_CELL_{i} }},\n",
            t = r.topic,
            f = r.fqn,
            m = r.min_rate_hz_milli,
            l = r.max_latency_ms,
            rt = r.route_trigger,
            rl = r.route_max_latency_ms,
            rd = r.route_deadline_ms,
            ri = r.reaction_id,
            mc = r.mechanism,
        ));
    }
    out.push_str("];\n\n");
    out.push_str(&format!("pub static {ages_name}: &[AgeMonitorSpec] = &[\n"));
    for (i, r) in ages.iter().enumerate() {
        out.push_str(&format!(
            "    AgeMonitorSpec {{ topic: {t:?}, fqn: {f:?}, max_age_ms: {a}u32, \
             deadline_ms: {d}u32, cell: &NROS_AGE_CELL_{i} }},\n",
            t = r.topic,
            f = r.fqn,
            a = r.max_age_ms,
            d = r.deadline_ms,
        ));
    }
    out.push_str("];\n\n");
    out
}

/// Where the rendered Rust names the monitor types and the executor.
///
/// The CLI bake writes a file the entry `include!`s with `nros-node` in scope
/// ([`RenderPaths::NROS_NODE`]); the `nros::main!` expansion reaches only the
/// `nros` umbrella ([`RenderPaths::NROS`]). The text is otherwise identical,
/// which is the point: one renderer, two roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPaths {
    /// Path of the module holding `MonitorSpec`, `AgeMonitorSpec`, the two
    /// cells and `MAX_MONITORS` / `MAX_AGE_MONITORS`.
    pub monitor: &'static str,
    /// Path of the `Executor` type.
    pub executor: &'static str,
}

impl RenderPaths {
    /// `nros-node`'s own paths — the baked `system_monitors.rs`.
    pub const NROS_NODE: Self = Self {
        monitor: "::nros_node::executor::monitor",
        executor: "::nros_node::executor::Executor",
    };
    /// The `nros` umbrella's re-exports — the `nros::main!` expansion.
    pub const NROS: Self = Self {
        monitor: "::nros::monitor",
        executor: "::nros::Executor",
    };
}

/// The endpoint ref's node half (`/ns/node/endpoint` -> `/ns/node`), the key a
/// per-executor slice keeps a row by. ONE spelling — the C/C++ lowering and the
/// `nros::main!` tier slice both ask it.
#[must_use]
pub fn row_node_fqn(endpoint_ref: &str) -> &str {
    endpoint_ref
        .rsplit_once('/')
        .map(|(node, _)| node)
        .unwrap_or("")
}

#[cfg(test)]
mod route_supervision_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn row(fqn: &str, route_ms: u32) -> MonitorRow {
        MonitorRow {
            topic: format!("{fqn}_topic"),
            fqn: fqn.into(),
            min_rate_hz_milli: 0,
            max_latency_ms: 0,
            route_trigger: if route_ms > 0 {
                "/in".into()
            } else {
                String::new()
            },
            route_max_latency_ms: route_ms,
            route_deadline_ms: 0,
            reaction: String::new(),
            reaction_id: 0,
            mechanism: 0,
        }
    }

    fn sup(deadline_ms: u32, reaction: &str, mechanism: &str) -> RouteSupervision {
        RouteSupervision {
            deadline_ms,
            reaction: reaction.into(),
            mechanism: mechanism_code(mechanism).expect("a mechanism"),
        }
    }

    /// phase-503 -- supervision lands on the route rows by output, reaction
    /// ids number the distinct reactions in sorted order from 1, and a row
    /// with no supervision stays as it was.
    #[test]
    fn supervision_attaches_and_numbers_reactions() {
        let mut rows = vec![row("/h/a", 149), row("/h/b", 149), row("/h/c", 149)];
        let mut s = BTreeMap::new();
        s.insert("/h/a".to_string(), sup(149, "/h/stop_now", "timer"));
        s.insert("/h/b".to_string(), sup(200, "/h/comfortable", "executor"));
        apply_route_supervision(&mut rows, &s).expect("applied");
        assert_eq!(
            reaction_ids(&rows),
            [
                (1, "/h/comfortable".to_string()),
                (2, "/h/stop_now".to_string())
            ]
        );
        assert_eq!(
            (
                rows[0].route_deadline_ms,
                rows[0].reaction_id,
                rows[0].mechanism
            ),
            (149, 2, MECHANISM_TIMER)
        );
        assert_eq!(
            (
                rows[1].route_deadline_ms,
                rows[1].reaction_id,
                rows[1].mechanism
            ),
            (200, 1, MECHANISM_EXECUTOR)
        );
        assert_eq!(rows[2], row("/h/c", 149), "unsupervised: unchanged");
        assert_eq!(mechanism_code("watchdog"), None);
    }

    /// A supervision for an output with no route row has nothing to watch.
    #[test]
    fn supervision_without_a_route_is_refused() {
        let mut rows = vec![row("/h/a", 0)];
        let mut s = BTreeMap::new();
        s.insert("/h/a".to_string(), sup(149, "/h/stop_now", "timer"));
        let err = apply_route_supervision(&mut rows, &s)
            .unwrap_err()
            .to_string();
        assert!(err.contains("/h/a"), "{err}");
    }

    /// An unsupervised row serialises exactly as it did before the fields.
    #[test]
    fn unsupervised_row_serialises_unchanged() {
        let json = serde_json::to_string(&row("/h/a", 149)).expect("json");
        assert!(!json.contains("route_deadline_ms"), "{json}");
        assert!(!json.contains("reaction"), "{json}");
        assert!(!json.contains("mechanism"), "{json}");
    }
}

#[cfg(test)]
mod hazard_row_tests {
    use super::*;
    use crate::ftti_budget::tests::{AGE_100, HAZARD, model_yaml, parse};

    /// The handler's stop path states a `safe_state` on `/handler/stop`.
    const SAFE_STATE: &str = "      safe_state:\n        emits: /handler/stop\n        \
                              settle_ms: 30.0\n";

    fn lowered(age: &str, hazard: &str) -> (Vec<MonitorRow>, Vec<AgeRow>, Vec<HazardRow>) {
        let m = parse(&model_yaml(age, SAFE_STATE, hazard, ""));
        let rows = monitor_rows(&m).expect("rows");
        let ages = age_rows(&m).expect("ages");
        let hz = hazard_rows(&m, &rows, &ages);
        (rows, ages, hz)
    }

    /// phase-462 W5b -- a contract `deadline` with no age bound still gets a
    /// row, carrying the deadline in wire milliseconds and no age bound, and it
    /// is NOT a hazard guard member: whether it judges anything depends on the
    /// backend, which the build does not know.
    #[test]
    fn a_contract_deadline_alone_gets_an_age_row_that_guards_nothing() {
        let (_, ages, hz) = lowered(
            "      qos:\n        deadline_ms: 50.0\n",
            &format!("{HAZARD}      ftti_ms: 500.0\n"),
        );
        assert_eq!(ages.len(), 1);
        assert_eq!((ages[0].max_age_ms, ages[0].deadline_ms), (0, 50));
        assert!(
            hz.iter()
                .flat_map(|h| &h.guards)
                .flat_map(|g| &g.members)
                .all(|m| !matches!(m, GuardMemberRow::Age(_))),
            "{hz:?}"
        );
    }

    /// The negative control: an age bound and a deadline share one row, and a
    /// contract that states neither gets none.
    #[test]
    fn a_deadline_rides_the_age_row_and_no_contract_means_no_row() {
        let (_, ages, _) = lowered(
            "      max_age_ms: 100.0\n      qos:\n        deadline_ms: 40.0\n",
            HAZARD,
        );
        assert_eq!(ages.len(), 1);
        assert_eq!((ages[0].max_age_ms, ages[0].deadline_ms), (100, 40));
        let (_, none, _) = lowered("      qos:\n        depth: 1\n", HAZARD);
        assert!(none.is_empty(), "{none:?}");
    }

    /// A guard topic with both an age and a rate contract is watched by both
    /// rows; the safe-state publisher gets a count-only row and is the sink.
    #[test]
    fn a_hazard_lowers_onto_the_rows_that_watch_its_guard() {
        let (rows, ages, hz) = lowered(AGE_100, &format!("{HAZARD}      ftti_ms: 500.0\n"));
        assert_eq!(hz.len(), 1);
        let h = &hz[0];
        assert_eq!(
            (h.key.as_str(), h.ftti_ms, h.settle_ms),
            ("demo.launch.xml/scan_lost", 500, 30)
        );
        let scan_rate = rows.iter().position(|r| r.fqn == "/sensor/scan").unwrap();
        assert_eq!(
            h.guards[0].members,
            vec![GuardMemberRow::Age(0), GuardMemberRow::Rate(scan_rate)]
        );
        assert_eq!(ages[0].fqn, "/handler/scan");
        // phase-462 W3c -- the stop path states a safe_state, so the hazard
        // carries its reaction id, in the model's one id space (no supervised
        // route here, so it is the only reaction: id 1).
        assert_eq!(h.reaction_id, 1);
        assert_eq!(
            model_reaction_ids(&parse(&model_yaml(AGE_100, SAFE_STATE, HAZARD, ""))).len(),
            1
        );
        let sink = h.sink.expect("sink row");
        assert_eq!(rows[sink].fqn, "/handler/stop");
        assert_eq!(
            (rows[sink].min_rate_hz_milli, rows[sink].max_latency_ms),
            (0, 20),
            "the sink row carries no rate promise (its latency is the path's own)"
        );
    }

    /// The shape play_launch resolves a contract to: the hazard reacts by a
    /// SCOPE path, and the safe state is on the node path at its sink. The
    /// hazard takes that node path's settle and publisher, and the scope path
    /// is its reaction. Before, both read the scope path's own safe state,
    /// which it never has, and the row baked settle 0 and no sink.
    #[test]
    fn a_scope_path_reaction_takes_the_safe_state_at_its_sink() {
        let lowered_scope = |output: &str| {
            let yaml = model_yaml(
                AGE_100,
                SAFE_STATE,
                "      guards:\n        - members: [/scan]\n      on: omission\n      \
                 reaction: demo.launch.xml/stop\n      ftti_ms: 500.0\n",
                "",
            )
            .replace(
                "\n  hazards:\n",
                &format!(
                    "\n  scope_paths:\n    demo.launch.xml/stop:\n      input: [/scan]\n      \
                     output: [{output}]\n      max_latency_ms: 50.0\n  hazards:\n"
                ),
            );
            let m = parse(&yaml);
            let rows = monitor_rows(&m).expect("rows");
            let ages = age_rows(&m).expect("ages");
            let hz = hazard_rows(&m, &rows, &ages);
            (m, rows, hz)
        };
        let (m, rows, hz) = lowered_scope("/stop");
        let h = &hz[0];
        assert_eq!(h.settle_ms, 30, "{h:?}");
        assert_eq!(rows[h.sink.expect("sink row")].fqn, "/handler/stop");
        let ids = model_reaction_ids(&m);
        assert_eq!(ids, vec![(1, "demo.launch.xml/stop".to_string())]);
        assert_eq!(h.reaction_id, 1);
        // The negative control: a scope path ending where no safe state is
        // published has nothing at its sink.
        let (m, _, hz) = lowered_scope("/scan");
        assert_eq!(
            (hz[0].settle_ms, hz[0].sink, hz[0].reaction_id),
            (0, None, 0)
        );
        assert!(model_reaction_ids(&m).is_empty());
    }

    /// play_launch phase 96 W4.2 -- the model's `rungs` (top first) reach
    /// the row: the top rung whose output this table set publishes, its route
    /// rounded up, and the rendered spec carries both. A rung on a topic no
    /// row publishes is passed over; a top observed rung with no route judges
    /// nothing (not the rung below it).
    #[test]
    fn the_top_observed_rung_and_its_route_reach_the_row() {
        let rungs = |list: &str| format!("{HAZARD}      ftti_ms: 500.0\n      rungs:\n{list}");
        // `comfort` ends on a topic nothing here publishes; `stop` on /stop.
        let (rows, ages, hz) = lowered(
            AGE_100,
            &rungs(concat!(
                "        - rung: comfort\n",
                "          role: rung\n",
                "          route_ms: 4.0\n",
                "          output: [/elsewhere]\n",
                "        - rung: stop\n",
                "          role: floor\n",
                "          route_ms: 7.5\n",
                "          starts_at: /handler\n",
                "          output: [/stop]\n",
            )),
        );
        let r = hz[0].rung.as_ref().expect("a rung is judged");
        assert_eq!((r.name.as_str(), r.route_ms), ("stop", 8), "{r:?}");
        assert_eq!(rows[r.row].topic, "/stop");
        let text = render_monitor_rs_full(&rows, &ages, &hz, RenderPaths::NROS_NODE);
        assert!(
            text.contains(&format!("rung_row: {}u16, rung_route_ms: 8u32", r.row)),
            "{text}"
        );

        // The top observed rung has no route: nothing is judged.
        let (_, _, hz) = lowered(
            AGE_100,
            &rungs("        - rung: stop\n          role: floor\n          output: [/stop]\n"),
        );
        assert_eq!(hz[0].rung, None);
        // No rungs at all (a model before play_launch phase 96).
        let (rows, ages, hz) = lowered(AGE_100, HAZARD);
        assert_eq!(hz[0].rung, None);
        let text = render_monitor_rs_full(&rows, &ages, &hz, RenderPaths::NROS_NODE);
        assert!(text.contains("rung_route_ms: 0u32"), "{text}");
    }

    /// A table set holding only part of a hazard's monitors does not lower
    /// it: another executor sees the rest, and two partial observers would
    /// report one hazard twice.
    #[test]
    fn a_hazard_split_across_tables_is_not_lowered_here() {
        let m = parse(&model_yaml(AGE_100, SAFE_STATE, HAZARD, ""));
        let rows = monitor_rows(&m).expect("rows");
        // This executor runs only the sensor: the age row is elsewhere.
        assert!(hazard_rows(&m, &rows, &[]).is_empty());
    }

    /// A model with no hazard renders exactly what it rendered before W3b.
    #[test]
    fn no_hazard_renders_the_old_include() {
        let rows = Vec::new();
        let ages = Vec::new();
        assert_eq!(
            render_monitor_rs(&rows, &ages),
            render_monitor_rs_full(&rows, &ages, &[], RenderPaths::NROS_NODE)
        );
        assert!(!render_monitor_rs(&rows, &ages).contains("set_hazard_table"));
    }

    /// The rendered include installs the hazard table after the rows it reads.
    #[test]
    fn the_installer_sets_the_hazard_table_last() {
        let (rows, ages, hz) = lowered(AGE_100, HAZARD);
        let text = render_monitor_rs_full(&rows, &ages, &hz, RenderPaths::NROS_NODE);
        assert!(
            text.contains("static NROS_HAZARD_CELL_0: HazardCell = HazardCell::new();"),
            "{text}"
        );
        assert!(text.contains("GuardRef::Age(0u16)"), "{text}");
        let age = text.find("set_age_table").unwrap();
        let hzd = text.find("set_hazard_table(NROS_HAZARDS)").unwrap();
        assert!(hzd > age, "{text}");
    }
}
