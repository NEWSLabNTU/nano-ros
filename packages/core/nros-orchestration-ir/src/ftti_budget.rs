//! phase-462 W3a -- a hazard's fault-tolerant time interval (FTTI) is checked
//! against what the IMAGE will run, at build time.
//!
//! rlm checks `ftti` against the contract's own declared latencies. This is
//! the other half: the bake knows how fast the executor can SEE a fault (the
//! monitor rows it bakes), how long the reaction path may take (its
//! `max_latency_ms`) and how long that path may wait to be released (its timer
//! period, or the period of the sched task that runs it). When those three add up to
//! more than the hazard's `ftti_ms`, the image cannot keep the promise, and
//! that is a build error naming every term -- the same fail-loud shape as
//! [`crate::qos_agreement`] and [`crate::violation_agreement`].
//!
//! # What the model carries, and what this reads
//!
//! The phase document sketched hazards before the model this tree pins
//! (ros-launch-manifest `contracts.hazards`) had its present shape. This
//! lowers what is THERE:
//!
//! - `contracts.hazards.<scope>/<name>`: `monitors` (each one fault: topic
//!   FQNs, faulting when ANY member does, or every member when `all_of`),
//!   `on` (the fault classes; empty means rlm's `FaultKind::DEFAULT`),
//!   `ftti_ms`, and `reaction` -- a key into `contracts.modes` (the mode the
//!   hazard takes away; each level of its `fallback` runs by its own
//!   `reaction`), else a path key directly (one level).
//! - `degradation_levels`: the levels the hazard runs, as the resolver
//!   wrote them (skipped levels absent), each with its charged `route_ms`.
//! - a level's mode: `min_dwell` (its charged dwell, a range's `max`) and
//!   `within_ms` (its own deadline, rlm v0.3.18).
//! - the reaction path is a `contracts.scope_paths` entry, or a
//!   `contracts.node_paths` one.
//!
//! # Every level, every deadline (rlm v0.3.18)
//!
//! rlm's "Which levels a hazard runs, and which rows are judged" is the one
//! statement both realizers implement. Each level the hazard runs has one
//! row: detection + every dwelling level run above it (its route plus its
//! charged dwell) + the level's route + the settle of the safe state that
//! route commands. A level without a dwell, the minimal risk condition
//! included, is judged against the hazard's `ftti` and against its own
//! `within`; a dwelling level against its `within` only. The judging and
//! the binding row (the largest share of its deadline) are
//! `ros_launch_manifest_derive::levels`, the code play_launch's checker
//! calls too, so the two report the same binding row on the same totals.
//! The route is the resolver's `degradation_levels[].route_ms` when the
//! model carries it (the quantity the image's rung monitor judges at run
//! time), else the level path's `max_latency_ms` plus its release wait.
//!
//! # Detection: only what the executor itself observes
//!
//! A fault is detected on target by a monitor row this image bakes on the
//! guard's topic ([`crate::contract_monitors`]):
//!
//! | fault class        | rule                  | bound                       |
//! | ---                | ---                   | ---                         |
//! | omission, loss     | `silence-runtime`     | the subscriber's `max_age_ms` |
//! | omission, loss     | `min-rate-runtime`    | two rate windows ([`RATE_DETECTION_MS`](crate::ftti_budget::RATE_DETECTION_MS)) |
//! | late               | `max-age-runtime`     | the subscriber's `max_age_ms` |
//!
//! The fastest applicable row wins for each member. `overrun`, `reported`
//! and rlm v0.3.13's `repetition`, `resequencing` and `insertion` have no
//! executor-side detector on a guard topic (a `reported` fault is the
//! application's own diagnostic), so a class with no row is UNOBSERVED here:
//! it is listed on the hazard's budget and never counted -- the Linux side,
//! or the application, may observe it, and a check that guessed would refuse
//! a correct system or pass a broken one.
//!
//! # When this abstains
//!
//! A level whose row has an unknown term (no observed detector, a route with
//! no `max_latency_ms` and none from the resolver, or a dwelling level above
//! it with an unknown route) and a level with no deadline have nothing to
//! judge, and are reported as such rather than refused: the refusal is
//! reserved for a deadline that is PROVABLY missed.

use std::{collections::BTreeMap, fmt};

use ros_launch_manifest_derive::levels::{self, Role, Row};
/// The judged pair and its deadline kind, as rlm states them (re-exported so
/// a reader of [`HazardBudget::binding`] needs no rlm dependency of its own).
pub use ros_launch_manifest_derive::levels::{Deadline, Judgement};
use ros_launch_manifest_model::{
    FaultKind, HazardContract, LevelRole, ModeContract, PathContract, SystemModel,
};
use ros_launch_manifest_sched::EffectiveTrigger;

/// The worst-case detection latency of a `min-rate-runtime` row, ms.
///
/// The rate check judges a whole window (`nros-node`'s
/// `monitor::RATE_CHECK_INTERVAL_US`, 5 s): a stream that stops after it has
/// already met its quota for the current window is only judged short when the
/// NEXT window closes, so the worst case is two windows. This crate cannot
/// depend on the `no_std` executor, so the number is restated here and a test
/// in `nros-node` pins the two together.
pub const RATE_DETECTION_MS: f64 = 2.0 * 5_000.0;

/// How one guard member's fault reaches the drain.
#[derive(Debug, Clone, PartialEq)]
pub struct Detector {
    /// The guard topic.
    pub topic: String,
    /// The executor rule that fires.
    pub rule: &'static str,
    /// The endpoint whose row carries it.
    pub endpoint: String,
    /// Worst-case detection latency, ms.
    pub bound_ms: f64,
}

/// The reaction term of a level: the path it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    /// The path the level's reaction resolves to.
    pub path: String,
    /// The mode it was reached through, when the hazard enters one.
    pub via_mode: Option<String>,
    /// The path's `max_latency_ms`; `None` when it states none.
    pub max_latency_ms: Option<f64>,
    /// How long the path may wait to be released, ms, and what says so.
    pub release: Option<(f64, String)>,
}

impl Reaction {
    /// The path's own route, ms: `max_latency_ms` plus its release wait.
    /// `None` when the path states no latency.
    pub fn own_route_ms(&self) -> Option<f64> {
        Some(self.max_latency_ms? + self.release.as_ref().map_or(0.0, |(ms, _)| *ms))
    }
}

/// Where a level's route term comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSource {
    /// The resolver's charged route for the level
    /// (`hazards.<h>.degradation_levels[].route_ms`): detection fire to the
    /// level's first output, the same quantity the image's rung monitor
    /// judges at run time.
    Model,
    /// The level's path: `max_latency_ms` plus its release wait (a model with
    /// no `degradation_levels` entry for the level).
    Path,
}

/// One level of a hazard's fallback sequence, as this image charges it
/// (rlm v0.3.18, "Which levels a hazard runs, and which rows are judged").
#[derive(Debug, Clone, PartialEq)]
pub struct LevelBudget {
    /// The level, as the model's `degradation_levels` names it (a mode
    /// name), or the path key of a one-level reaction.
    pub level: String,
    /// Level, dwelling level or minimal risk condition.
    pub role: LevelRole,
    /// The level's reaction path; `None` when the level names none.
    pub reaction: Option<Reaction>,
    /// The route charged, ms, and where it comes from; `None` when unknown.
    pub route: Option<(f64, RouteSource)>,
    /// The settle of the safe state the level's route commands, ms (0 when
    /// it commands none).
    pub settle_ms: f64,
    /// Every dwelling level run above this one, each its route plus its
    /// charged dwell, ms; `None` when one of those routes is unknown.
    pub waited_ms: Option<f64>,
    /// This level's charged dwell (a dwell range's `max`, else its single
    /// duration), when it dwells.
    pub dwell_ms: Option<f64>,
    /// The level's own deadline (`modes.<m>.within`), ms.
    pub within_ms: Option<f64>,
    /// Fault to the level entered, ms: detection + waited + route + settle.
    /// `None` when any term is unknown: the level is then not judged.
    pub total_ms: Option<f64>,
}

impl LevelBudget {
    fn role(&self) -> Role {
        match self.role {
            LevelRole::Level => Role::Level,
            LevelRole::Dwell => Role::Dwell,
            LevelRole::Mrc => Role::Mrc,
        }
    }
}

/// One hazard's budget, as this image runs it.
#[derive(Debug, Clone, PartialEq)]
pub struct HazardBudget {
    /// `contracts.hazards` key.
    pub hazard: String,
    /// `ftti_ms`, when declared.
    pub ftti_ms: Option<f64>,
    /// The slowest detector the budget must allow for, when any is observed.
    pub detection: Option<Detector>,
    /// `(topic, class)` pairs with no executor-side detector in this image.
    pub unobserved: Vec<(String, FaultKind)>,
    /// The minimal risk condition's reaction, when the hazard names one.
    pub reaction: Option<Reaction>,
    /// Every level the hazard runs, the first level of the fallback sequence
    /// first; the last is the minimal risk condition.
    pub levels: Vec<LevelBudget>,
}

impl HazardBudget {
    /// The minimal risk condition's row total, ms -- `None` when any term is
    /// unknown.
    pub fn total_ms(&self) -> Option<f64> {
        self.levels
            .iter()
            .rfind(|l| l.role == LevelRole::Mrc)
            .and_then(|l| l.total_ms)
    }

    /// The rows the judged pairs are chosen from: every level whose total
    /// is known, and the index of its [`LevelBudget`].
    fn rows(&self) -> (Vec<Row>, Vec<usize>) {
        self.levels
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                Some((
                    Row {
                        level: l.level.clone(),
                        role: l.role(),
                        total_ms: l.total_ms?,
                        within_ms: l.within_ms,
                    },
                    i,
                ))
            })
            .unzip()
    }

    /// Every judged (level, deadline) pair (rlm v0.3.18,
    /// `ros_launch_manifest_derive::levels::judgements`): a level without a
    /// dwell against `ftti` and its own `within`, a dwelling level against
    /// its `within` only. Each pair's `row` indexes [`Self::levels`].
    pub fn judgements(&self) -> Vec<Judgement> {
        let (rows, at) = self.rows();
        levels::judgements(&rows, self.ftti_ms)
            .into_iter()
            .map(|mut j| {
                j.row = at[j.row];
                j
            })
            .collect()
    }

    /// The binding pair: the largest share of its deadline (rlm v0.3.18,
    /// `ros_launch_manifest_derive::levels::binding`). Its `row` indexes
    /// [`Self::levels`].
    pub fn binding(&self) -> Option<Judgement> {
        let (rows, at) = self.rows();
        levels::binding(&rows, self.ftti_ms).map(|mut j| {
            j.row = at[j.row];
            j
        })
    }

    /// The judged pairs this image provably misses.
    pub fn misses(&self) -> Vec<Judgement> {
        self.judgements()
            .into_iter()
            .filter(|j| j.total_ms > j.deadline_ms + 1e-9)
            .collect()
    }

    /// True when a judged pair is provably missed.
    pub fn misses_a_deadline(&self) -> bool {
        !self.misses().is_empty()
    }
}

fn deadline_name(d: Deadline) -> &'static str {
    match d {
        Deadline::Ftti => "ftti",
        Deadline::Within => "within",
    }
}

fn role_name(r: LevelRole) -> &'static str {
    match r {
        LevelRole::Level => "level",
        LevelRole::Dwell => "dwell",
        LevelRole::Mrc => "mrc",
    }
}

impl fmt::Display for HazardBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = |v: f64| format!("{v:.1} ms");
        write!(f, "hazard `{}`:", self.hazard)?;
        match self.ftti_ms {
            Some(ftti) => write!(f, " ftti {}", ms(ftti))?,
            None => write!(f, " no ftti_ms")?,
        }
        if let Some(j) = self.binding() {
            let l = &self.levels[j.row];
            write!(
                f,
                "; binding level `{}` ({}): the image needs {} against {} {} ({:.0}%)",
                l.level,
                role_name(l.role),
                ms(j.total_ms),
                deadline_name(j.deadline),
                ms(j.deadline_ms),
                100.0 * j.share()
            )?;
        }
        writeln!(f)?;
        match &self.detection {
            Some(d) => writeln!(
                f,
                "  detection {}: `{}` on `{}` (row of `{}`)",
                ms(d.bound_ms),
                d.rule,
                d.topic,
                d.endpoint
            )?,
            None => writeln!(f, "  detection: no executor-side detector in this image")?,
        }
        if self.levels.is_empty() {
            writeln!(f, "  reaction: none named")?;
        }
        let misses = self.misses();
        for (i, l) in self.levels.iter().enumerate() {
            write!(f, "  level `{}` ({})", l.level, role_name(l.role))?;
            match l.total_ms {
                Some(t) => write!(f, " {}", ms(t))?,
                None => write!(f, " not judged (a term is unknown)")?,
            }
            if let Some(w) = l.within_ms {
                write!(f, ", within {}", ms(w))?;
            }
            for j in misses.iter().filter(|j| j.row == i) {
                write!(
                    f,
                    ", MISSES {} by {}",
                    deadline_name(j.deadline),
                    ms(-j.slack_ms())
                )?;
            }
            writeln!(f)?;
            if let Some(w) = l.waited_ms.filter(|w| *w > 0.0) {
                writeln!(f, "    waited {}: the dwelling levels above it", ms(w))?;
            }
            match (&l.reaction, l.route) {
                (Some(r), route) => {
                    let via = r
                        .via_mode
                        .as_ref()
                        .map(|m| format!(" (through mode `{m}`)"))
                        .unwrap_or_default();
                    match route {
                        Some((v, RouteSource::Model)) => writeln!(
                            f,
                            "    route {}: path `{}`{via}, the resolver's charged route",
                            ms(v),
                            r.path
                        )?,
                        Some((v, RouteSource::Path)) => {
                            writeln!(
                                f,
                                "    route {}: path `{}`{via} max_latency_ms",
                                ms(v),
                                r.path
                            )?;
                            if let Some((wait, why)) = &r.release {
                                writeln!(f, "    release {}: {why}", ms(*wait))?;
                            }
                        }
                        None => writeln!(
                            f,
                            "    route: path `{}`{via} states no max_latency_ms",
                            r.path
                        )?,
                    }
                }
                (None, _) => writeln!(f, "    route: the level names no reaction path")?,
            }
            if l.settle_ms > 0.0 {
                writeln!(f, "    settle {}", ms(l.settle_ms))?;
            }
            if let Some(d) = l.dwell_ms {
                writeln!(f, "    dwell {}: charged to every level below", ms(d))?;
            }
        }
        for (topic, kind) in &self.unobserved {
            writeln!(f, "  unobserved here: {kind:?} on `{topic}` (not counted)")?;
        }
        Ok(())
    }
}

/// Every way this check refuses.
#[derive(Debug, Clone, PartialEq)]
pub enum FttiBudgetError {
    /// The hazard's `reaction` is neither a mode nor a path in this model.
    UnknownReaction { hazard: String, reaction: String },
    /// A fallback level of the mode a hazard enters names a reaction that is
    /// no path.
    UnknownModeReaction {
        hazard: String,
        mode: String,
        level: String,
        reaction: String,
    },
    /// The model's `degradation_levels` names a level that is no mode.
    UnknownLevel { hazard: String, level: String },
    /// One or more hazards whose image misses a level's deadline.
    Exceeded(Vec<HazardBudget>),
}

impl fmt::Display for FttiBudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FttiBudgetError::UnknownReaction { hazard, reaction } => write!(
                f,
                "contract: `hazards.{hazard}.reaction` names `{reaction}`, which is neither a \
                 mode nor a path in this model, so the image cannot say what runs when the \
                 hazard is detected."
            ),
            FttiBudgetError::UnknownModeReaction {
                hazard,
                mode,
                level,
                reaction,
            } => write!(
                f,
                "contract: hazard `{hazard}` enters mode `{mode}`, whose fallback level \
                 `{level}` names `{reaction}` -- not a path in this model."
            ),
            FttiBudgetError::UnknownLevel { hazard, level } => write!(
                f,
                "model: `hazards.{hazard}.degradation_levels` names level `{level}`, which is \
                 not a mode in this model."
            ),
            FttiBudgetError::Exceeded(bs) => {
                writeln!(
                    f,
                    "{} hazard(s) cannot meet a deadline on this image: a level of the \
                     fallback sequence is entered later than the hazard's `ftti_ms` or the \
                     level's own `within` (detection, the dwelling levels waited above it, \
                     its route and its settle). Shorten a term (a tighter `max_age_ms` on the \
                     guard, a faster reaction, a shorter period or dwell) or relax the \
                     deadline.",
                    bs.len()
                )?;
                for b in bs {
                    writeln!(f)?;
                    write!(f, "{b}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for FttiBudgetError {}

/// The fastest detector this image bakes for `kind` on `topic`.
fn detector(model: &SystemModel, topic: &str, kind: FaultKind) -> Option<Detector> {
    let wiring = model.structure.topics.get(topic)?;
    let mut best: Option<Detector> = None;
    let mut offer = |d: Detector| {
        if best.as_ref().is_none_or(|b| d.bound_ms < b.bound_ms) {
            best = Some(d);
        }
    };
    let age_rule = match kind {
        FaultKind::Omission | FaultKind::Loss => Some("silence-runtime"),
        FaultKind::Late => Some("max-age-runtime"),
        // rlm v0.3.13's communication classes need the previous stamp of
        // each publisher (repetition, resequencing) or the set of publishers
        // the contract names (insertion); the executor tracks neither, so
        // they are UNOBSERVED here, listed and never counted.
        FaultKind::Reported
        | FaultKind::Overrun
        | FaultKind::Repetition
        | FaultKind::Resequencing
        | FaultKind::Insertion => None,
    };
    if let Some(rule) = age_rule {
        for sub in &wiring.subscribers {
            let Some(age) = model
                .contracts
                .sub_endpoints
                .get(sub)
                .and_then(|c| c.max_age_ms)
            else {
                continue;
            };
            offer(Detector {
                topic: topic.to_string(),
                rule,
                endpoint: sub.clone(),
                bound_ms: age.max(1.0),
            });
        }
    }
    if matches!(kind, FaultKind::Omission | FaultKind::Loss) {
        for p in &wiring.publishers {
            if model
                .contracts
                .pub_endpoints
                .get(p)
                .and_then(|c| c.min_rate_hz)
                .is_some()
            {
                offer(Detector {
                    topic: topic.to_string(),
                    rule: "min-rate-runtime",
                    endpoint: p.clone(),
                    bound_ms: RATE_DETECTION_MS,
                });
            }
        }
    }
    best
}

/// The node FQN a contract ref belongs to (`/a/node/ep` -> `/a/node`).
fn node_of(reference: &str) -> &str {
    reference
        .rsplit_once('/')
        .map(|(n, _)| n)
        .unwrap_or(reference)
}

/// How long `path` may wait to be released: a timer's period (plus its
/// stated jitter), else the period of the sched task that runs its first input's
/// node.
fn release(model: &SystemModel, path: &PathContract) -> Option<(f64, String)> {
    if let EffectiveTrigger::Timer { .. } = path.effective_trigger()
        && let Some(period) = path.effective_trigger().period_ms()
    {
        let jitter = path.timer_jitter_ms.unwrap_or(0.0);
        return Some((
            period + jitter,
            format!("its timer period ({period:.1} ms) plus stated jitter ({jitter:.1} ms)"),
        ));
    }
    let node = node_of(path.input.first()?);
    let sched_task = model
        .execution
        .bindings
        .iter()
        // A binding key is `<node FQN>` or `<node FQN>/<callback group>`, and a
        // node FQN holds slashes, so the model's own node list decides which
        // (the rule `violation_agreement` binds by).
        .find(|(k, _)| {
            let bound = if model.structure.nodes.contains_key(k.as_str()) {
                k.as_str()
            } else {
                node_of(k)
            };
            bound == node
        })
        .map(|(_, t)| t)?;
    let ms = model
        .execution
        .tasks
        .get(sched_task)?
        .period?
        .as_millis_f64();
    Some((
        ms,
        format!("sched task `{sched_task}`'s period, which releases node `{node}`"),
    ))
}

/// A hazard's resolved reaction: the path's key and contract, and the mode
/// it is reached through when the hazard enters one.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedReaction<'m> {
    /// The `scope_paths` / `node_paths` key.
    pub key: &'m str,
    /// That path's contract.
    pub path: &'m PathContract,
    /// The mode the hazard enters, when it reacts through one.
    pub via_mode: Option<&'m str>,
}

/// The path a hazard's `reaction` resolves to, and the mode it is reached
/// through when the hazard enters one -- `None` when the hazard names no
/// reaction, or enters a mode whose last fallback level names no path
/// (entering that level IS the reaction). One resolver, read by this check
/// and by the observer rows (`contract_monitors::hazard_rows`), so the two
/// cannot follow different reactions.
///
/// A mode is read as rlm v0.3.15 states it (`hazards.<h>.enters`, play_launch
/// phase 103 D4): "The mode this hazard takes away. The reaction is that
/// mode's `fallback:` sequence: each level in order, by its own `reaction:`,
/// each checked in its own right (`degradation-level-time`) and the minimal
/// risk condition (the last level) against `ftti`. The named mode's own
/// `reaction:`, if it has one, is not what this hazard runs." This resolves
/// the minimal risk condition, the reaction the observer rows start; the
/// budget charges every level ([`hazard_budgets`]), and the levels reach the
/// image as the hazard's rungs (`contract_monitors::hazard_rungs`). A mode
/// with no fallback has no reaction here.
pub fn reaction_path<'m>(
    model: &'m SystemModel,
    hazard: &str,
    h: &'m HazardContract,
) -> Result<Option<ResolvedReaction<'m>>, FttiBudgetError> {
    let Some(r) = h.reaction.as_deref() else {
        return Ok(None);
    };
    // A mode first, the way the checker resolves it (the model's own rule).
    let (key, via_mode): (&str, Option<&'m str>) = match model.contracts.modes.get(r) {
        Some(mode) => {
            let Some(mrc) = mode
                .fallback
                .last()
                .and_then(|level| model.contracts.modes.get(level))
            else {
                return Ok(None);
            };
            match mrc.reaction.as_deref() {
                Some(k) => (k, Some(r)),
                None => return Ok(None),
            }
        }
        None => (r, None),
    };
    let found = model
        .contracts
        .scope_paths
        .get_key_value(key)
        .or_else(|| model.contracts.node_paths.get_key_value(key));
    let Some((k, p)) = found else {
        return Err(match via_mode {
            Some(mode) => FttiBudgetError::UnknownModeReaction {
                hazard: hazard.to_string(),
                mode: mode.to_string(),
                level: model.contracts.modes[mode]
                    .fallback
                    .last()
                    .cloned()
                    .unwrap_or_default(),
                reaction: key.to_string(),
            },
            None => FttiBudgetError::UnknownReaction {
                hazard: hazard.to_string(),
                reaction: key.to_string(),
            },
        });
    };
    Ok(Some(ResolvedReaction {
        key: k.as_str(),
        path: p,
        via_mode,
    }))
}

/// A path key's contract: a `scope_paths` entry, else a `node_paths` one.
fn path_of<'m>(model: &'m SystemModel, key: &str) -> Option<(&'m str, &'m PathContract)> {
    model
        .contracts
        .scope_paths
        .get_key_value(key)
        .or_else(|| model.contracts.node_paths.get_key_value(key))
        .map(|(k, p)| (k.as_str(), p))
}

fn reaction_of(model: &SystemModel, key: &str, path: &PathContract, via: Option<&str>) -> Reaction {
    Reaction {
        path: key.to_string(),
        via_mode: via.map(str::to_string),
        max_latency_ms: path.max_latency_ms,
        release: release(model, path),
    }
}

/// The settle of the safe state `key`'s path commands, ms; 0 when none.
fn settle_ms(model: &SystemModel, key: &str, path: &PathContract, via: Option<&str>) -> f64 {
    let r = ResolvedReaction {
        key,
        path,
        via_mode: via,
    };
    crate::contract_monitors::reaction_safe_state(model, &r)
        .and_then(|ss| ss.settle_ms)
        .unwrap_or(0.0)
}

/// The mode a `degradation_levels[].level` names: the model writes the
/// level without the hazard's scope (`blind_stop`), and keys modes with it
/// (`<scope>/blind_stop`).
fn level_mode<'m>(model: &'m SystemModel, hazard: &str, level: &str) -> Option<&'m ModeContract> {
    let scope = hazard.rsplit_once('/').map_or("", |(s, _)| s);
    model
        .contracts
        .modes
        .get(&format!("{scope}/{level}"))
        .or_else(|| model.contracts.modes.get(level))
}

/// One level the hazard runs, before its row is added up.
struct Step<'m> {
    level: String,
    role: LevelRole,
    mode: Option<&'m ModeContract>,
    /// The level's reaction path key, when it names one.
    reaction: Option<&'m str>,
    /// The resolver's charged route, when the model carries it.
    route_ms: Option<f64>,
}

/// The levels a hazard runs, in order (rlm v0.3.18 steps 1 and 2):
///
/// - a hazard that enters a mode runs the levels of that mode's `fallback:`
///   it does not skip. Which it skips needs the functions the fault removes,
///   which is the resolver's analysis; the model records its answer as
///   `degradation_levels` ("a skipped level ... is not in the model's
///   `degradation_levels` for that hazard"), so this reads that list. A model
///   with none (one written by hand) has no skip answer, and is charged its
///   minimal risk condition only, which no fault skips;
/// - a hazard with `reaction: <path>` runs one level, its minimal risk
///   condition.
fn steps<'m>(
    model: &'m SystemModel,
    hazard: &str,
    h: &'m HazardContract,
) -> Result<Vec<Step<'m>>, FttiBudgetError> {
    let Some(r) = h.reaction.as_deref() else {
        return Ok(Vec::new());
    };
    let Some(entered) = model.contracts.modes.get(r) else {
        let lr = h.degradation_levels.first();
        return Ok(vec![Step {
            level: lr.map_or_else(|| r.to_string(), |l| l.level.clone()),
            role: LevelRole::Mrc,
            mode: None,
            reaction: Some(r),
            route_ms: lr.and_then(|l| l.route_ms),
        }]);
    };
    if !h.degradation_levels.is_empty() {
        return h
            .degradation_levels
            .iter()
            .map(|lr| {
                let mode = level_mode(model, hazard, &lr.level).ok_or_else(|| {
                    FttiBudgetError::UnknownLevel {
                        hazard: hazard.to_string(),
                        level: lr.level.clone(),
                    }
                })?;
                Ok(Step {
                    level: lr.level.clone(),
                    role: lr.role,
                    mode: Some(mode),
                    reaction: mode.reaction.as_deref(),
                    route_ms: lr.route_ms,
                })
            })
            .collect();
    }
    let Some((key, mrc)) = entered
        .fallback
        .last()
        .and_then(|level| model.contracts.modes.get_key_value(level))
    else {
        return Ok(Vec::new());
    };
    Ok(vec![Step {
        level: key.clone(),
        role: LevelRole::Mrc,
        mode: Some(mrc),
        reaction: mrc.reaction.as_deref(),
        route_ms: None,
    }])
}

/// Every level's row (rlm v0.3.18 step 3): detection + every dwelling level
/// run above it (its route plus its charged dwell) + the level's route + the
/// settle of the safe state that route commands.
fn level_budgets(
    model: &SystemModel,
    hazard: &str,
    h: &HazardContract,
    detection_ms: Option<f64>,
) -> Result<Vec<LevelBudget>, FttiBudgetError> {
    let via = h
        .reaction
        .as_deref()
        .filter(|r| model.contracts.modes.contains_key(*r));
    let mut out = Vec::new();
    let mut waited = Some(0.0_f64);
    for s in steps(model, hazard, h)? {
        let path = match s.reaction {
            Some(k) => match path_of(model, k) {
                Some(p) => Some(p),
                None => {
                    return Err(match via {
                        Some(mode) => FttiBudgetError::UnknownModeReaction {
                            hazard: hazard.to_string(),
                            mode: mode.to_string(),
                            level: s.level.clone(),
                            reaction: k.to_string(),
                        },
                        None => FttiBudgetError::UnknownReaction {
                            hazard: hazard.to_string(),
                            reaction: k.to_string(),
                        },
                    });
                }
            },
            None => None,
        };
        let reaction = path.map(|(k, p)| reaction_of(model, k, p, via));
        let route = match (s.route_ms, &reaction) {
            (Some(v), _) => Some((v, RouteSource::Model)),
            (None, Some(r)) => r.own_route_ms().map(|v| (v, RouteSource::Path)),
            (None, None) => None,
        };
        let settle = path.map_or(0.0, |(k, p)| settle_ms(model, k, p, via));
        let dwell_ms = (s.role == LevelRole::Dwell)
            .then(|| s.mode.and_then(|m| m.min_dwell.as_ref()))
            .flatten()
            .map(|d| d.charged_ms());
        let waited_above = waited;
        let total_ms = match (detection_ms, waited_above, route) {
            (Some(d), Some(w), Some((r, _))) => Some(d + w + r + settle),
            _ => None,
        };
        let within_ms = s.mode.and_then(|m| m.within_ms);
        if s.role == LevelRole::Dwell {
            waited = match (waited, route) {
                (Some(w), Some((r, _))) => Some(w + r + dwell_ms.unwrap_or(0.0)),
                _ => None,
            };
        }
        out.push(LevelBudget {
            level: s.level,
            role: s.role,
            reaction,
            route,
            settle_ms: settle,
            waited_ms: waited_above,
            dwell_ms,
            within_ms,
            total_ms,
        });
    }
    Ok(out)
}

/// Every hazard's budget, as this image runs it. Refuses only a reaction the
/// model cannot resolve; a missed deadline is [`check_model`]'s verdict.
pub fn hazard_budgets(model: &SystemModel) -> Result<Vec<HazardBudget>, FttiBudgetError> {
    let mut out = Vec::new();
    for (key, h) in &model.contracts.hazards {
        let mut detection: Option<Detector> = None;
        let mut unobserved = Vec::new();
        // rlm v0.3.15 (`FaultKind::DEFAULT`, play_launch phase 103 D5): "The
        // classes an omitted `on:` means: the five the grammar had before
        // v0.3.13. ... This is the one definition: the Linux and RTOS
        // realizers both read it."
        for kind in h.classes() {
            for guard in &h.monitors {
                for member in &guard.members {
                    match detector(model, member, kind) {
                        // A guard faults on any member (or only when all of
                        // them do); either way the member that faults may be
                        // the slowest to detect, so the budget allows for it.
                        Some(d) => {
                            if detection.as_ref().is_none_or(|w| d.bound_ms > w.bound_ms) {
                                detection = Some(d);
                            }
                        }
                        None => unobserved.push((member.clone(), kind)),
                    }
                }
            }
        }
        let levels = level_budgets(model, key, h, detection.as_ref().map(|d| d.bound_ms))?;
        let reaction = levels
            .iter()
            .rfind(|l| l.role == LevelRole::Mrc)
            .and_then(|l| l.reaction.clone());
        out.push(HazardBudget {
            hazard: key.clone(),
            ftti_ms: h.ftti_ms,
            detection,
            unobserved,
            reaction,
            levels,
        });
    }
    Ok(out)
}

/// The build-time verdict: every hazard of which this image provably misses
/// a judged pair (rlm v0.3.18 step 4: a level without a dwell against `ftti`
/// and its own `within`, a dwelling level against its `within`), as one
/// refusal naming each term.
pub fn check_model(model: &SystemModel) -> Result<Vec<HazardBudget>, FttiBudgetError> {
    let budgets = hazard_budgets(model)?;
    let over: Vec<HazardBudget> = budgets
        .iter()
        .filter(|b| b.misses_a_deadline())
        .cloned()
        .collect();
    if over.is_empty() {
        Ok(budgets)
    } else {
        Err(FttiBudgetError::Exceeded(over))
    }
}

/// Budgets by hazard key, for a reader that joins them to its own rows.
pub fn budgets_by_hazard(budgets: &[HazardBudget]) -> BTreeMap<&str, &HazardBudget> {
    budgets.iter().map(|b| (b.hazard.as_str(), b)).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A sensor publishing `/scan`, a handler subscribing it, and a hazard on
    /// `/scan` whose reaction is the handler's stop path. `extra` lands under
    /// `contracts:`; `hazard` is the hazard body; `sched_tasks` goes under
    /// `execution:`.
    pub(crate) fn model_yaml(
        sub_age: &str,
        path_extra: &str,
        hazard: &str,
        sched_tasks: &str,
    ) -> String {
        format!(
            "meta:\n  version: 1\n\
             structure:\n  scopes:\n    demo.launch.xml:\n      file: demo.launch.xml\n  \
             nodes:\n    /sensor:\n      scope: demo.launch.xml\n      pkg: demo\n      \
             exec: sensor\n      node_name: sensor\n      namespace: /\n    \
             /handler:\n      scope: demo.launch.xml\n      pkg: demo\n      \
             exec: handler\n      node_name: handler\n      namespace: /\n  \
             topics:\n    /scan:\n      type: std_msgs/msg/Empty\n      pub: [/sensor/scan]\n      \
             sub: [/handler/scan]\n    /stop:\n      type: std_msgs/msg/Empty\n      pub: [/handler/stop]\n\
             contracts:\n  pub_endpoints:\n    /sensor/scan:\n      min_rate_hz: 10.0\n  \
             sub_endpoints:\n    /handler/scan:\n{sub_age}\
             \n  node_paths:\n    /handler/stop_path:\n      input: [/handler/scan]\n      \
             output: [/handler/stop]\n      max_latency_ms: 20.0\n{path_extra}\
             \n  hazards:\n    demo.launch.xml/scan_lost:\n{hazard}\
             \nexecution:\n  bindings:\n    /handler: rt\n  tasks:\n    rt:\n      \
             class: real_time\n{sched_tasks}"
        )
    }

    pub(crate) fn parse(yaml: &str) -> SystemModel {
        SystemModel::from_yaml_str(yaml).expect("fixture parses")
    }

    pub(crate) const AGE_100: &str = "      max_age_ms: 100.0\n";
    pub(crate) const HAZARD: &str = "      monitors:\n        - members: [/scan]\n      on: omission\n      \
                          reaction: /handler/stop_path\n";

    fn hazard_with_ftti(ftti: &str) -> String {
        format!("{HAZARD}      ftti_ms: {ftti}\n")
    }

    /// The terms add up the way the module says: silence on the guard (100)
    /// + the reaction path (20) + its sched task's period (10).
    #[test]
    fn a_budget_is_detection_plus_reaction_plus_release() {
        let m = parse(&model_yaml(
            AGE_100,
            "",
            &hazard_with_ftti("500.0"),
            "      period: 10000us\n",
        ));
        let b = &hazard_budgets(&m).expect("resolves")[0];
        let d = b.detection.as_ref().expect("observed");
        assert_eq!((d.rule, d.bound_ms), ("silence-runtime", 100.0));
        assert_eq!(b.total_ms(), Some(130.0));
        assert!(!b.misses_a_deadline());
        check_model(&m).expect("130 ms fits a 500 ms FTTI");
    }

    /// The negative control: the same image with an FTTI below the sum
    /// refuses, naming every term.
    #[test]
    fn an_ftti_below_the_chain_refuses_naming_each_term() {
        let m = parse(&model_yaml(
            AGE_100,
            "",
            &hazard_with_ftti("120.0"),
            "      period: 10000us\n",
        ));
        let err = check_model(&m).expect_err("130 > 120 must refuse");
        let text = err.to_string();
        for needle in [
            "scan_lost",
            "ftti 120.0 ms",
            "130.0 ms",
            "silence-runtime",
            "/handler/scan",
            "/handler/stop_path",
            "sched task `rt`",
        ] {
            assert!(text.contains(needle), "names `{needle}`: {text}");
        }
    }

    /// With no age contract the only detector is the publisher's rate row,
    /// which needs two windows -- a 500 ms FTTI cannot be kept by it.
    #[test]
    fn a_rate_only_guard_is_charged_two_windows() {
        let m = parse(&model_yaml(
            "      {}\n",
            "",
            &hazard_with_ftti("500.0"),
            "",
        ));
        let b = &hazard_budgets(&m).expect("resolves")[0];
        let d = b.detection.as_ref().expect("observed by rate");
        assert_eq!(
            (d.rule, d.bound_ms),
            ("min-rate-runtime", RATE_DETECTION_MS)
        );
        check_model(&m).expect_err("10 s of detection exceeds 500 ms");
    }

    /// A timer-triggered reaction waits for its tick: period plus jitter.
    #[test]
    fn a_timer_reaction_waits_its_period_and_jitter() {
        let m = parse(&model_yaml(
            AGE_100,
            "      trigger:\n        kind: timer\n        value:\n          rate_hz: 50.0\n      \
             timer_jitter_ms: 2.0\n",
            &hazard_with_ftti("500.0"),
            "",
        ));
        let b = &hazard_budgets(&m).expect("resolves")[0];
        let (wait, why) = b.reaction.as_ref().unwrap().release.clone().unwrap();
        assert_eq!(wait, 22.0, "{why}");
        assert_eq!(b.total_ms(), Some(142.0));
    }

    /// A class nothing in the image detects is listed, never counted.
    #[test]
    fn a_reported_fault_is_unobserved_not_counted() {
        let hazard = HAZARD.replace("on: omission", "on: [omission, reported]");
        let m = parse(&model_yaml(
            AGE_100,
            "",
            &format!("{hazard}      ftti_ms: 500.0\n"),
            "",
        ));
        let b = &hazard_budgets(&m).expect("resolves")[0];
        assert_eq!(
            b.unobserved,
            vec![("/scan".to_string(), FaultKind::Reported)]
        );
        assert_eq!(b.total_ms(), Some(120.0));
    }

    /// rlm v0.3.13's communication classes have no detector on the image:
    /// named in `on:`, each is listed as unobserved, never counted.
    #[test]
    fn the_communication_classes_are_unobserved() {
        let hazard = HAZARD.replace(
            "on: omission",
            "on: [omission, repetition, resequencing, insertion]",
        );
        let m = parse(&model_yaml(
            AGE_100,
            "",
            &format!("{hazard}      ftti_ms: 500.0\n"),
            "",
        ));
        let b = &hazard_budgets(&m).expect("resolves")[0];
        assert_eq!(
            b.unobserved,
            vec![
                ("/scan".to_string(), FaultKind::Repetition),
                ("/scan".to_string(), FaultKind::Resequencing),
                ("/scan".to_string(), FaultKind::Insertion),
            ]
        );
        assert_eq!(b.total_ms(), Some(120.0), "omission is still counted");
    }

    /// No FTTI, or a reaction with no latency: nothing to add up, so no
    /// refusal -- only a provable miss refuses.
    #[test]
    fn an_unbounded_term_abstains() {
        let m = parse(&model_yaml(AGE_100, "", HAZARD, ""));
        check_model(&m).expect("no ftti_ms: abstains");
        let m = parse(
            &model_yaml(AGE_100, "", &hazard_with_ftti("1.0"), "")
                .replace("      max_latency_ms: 20.0\n", ""),
        );
        let b = &check_model(&m).expect("no reaction latency: abstains")[0];
        assert_eq!(b.total_ms(), None);
    }

    /// A reaction that is neither a mode nor a path is a model the image
    /// cannot run, and refuses.
    #[test]
    fn an_unknown_reaction_refuses() {
        let hazard = HAZARD.replace("/handler/stop_path", "/handler/nowhere");
        let m = parse(&model_yaml(AGE_100, "", &hazard, ""));
        let err = hazard_budgets(&m).expect_err("must refuse");
        assert!(err.to_string().contains("/handler/nowhere"), "{err}");
    }

    /// A hazard that enters a mode reacts through the mode's fallback, and
    /// the budget charges its minimal risk condition (the last level), not
    /// the entered mode's own `reaction` (rlm v0.3.15, play_launch phase 103
    /// D4). The entered mode here names a reaction of its own, a path with
    /// no budget; reading it, as the image did before, charges nothing.
    #[test]
    fn a_mode_reaction_resolves_through_the_fallback() {
        let hazard = HAZARD.replace("/handler/stop_path", "demo.launch.xml/driving");
        let drive = "    /handler/drive_path:\n      input: [/handler/scan]\n      \
                     output: [/handler/stop]\n";
        let yaml = model_yaml(
            AGE_100,
            drive,
            &format!("{hazard}      ftti_ms: 500.0\n"),
            "",
        )
        .replace(
            "\n  hazards:",
            "\n  modes:\n    demo.launch.xml/driving:\n      \
                 reaction: /handler/drive_path\n      \
                 fallback: [demo.launch.xml/slow, demo.launch.xml/safe_stop]\n    \
             demo.launch.xml/slow:\n      reaction: /handler/drive_path\n    \
             demo.launch.xml/safe_stop:\n      reaction: /handler/stop_path\n  hazards:",
        );
        let m = parse(&yaml);
        let b = &hazard_budgets(&m).expect("resolves")[0];
        let r = b.reaction.as_ref().expect("reaction");
        assert_eq!(r.path, "/handler/stop_path");
        assert_eq!(r.max_latency_ms, Some(20.0));
        assert_eq!(r.via_mode.as_deref(), Some("demo.launch.xml/driving"));
    }

    /// A mode with no fallback takes away nothing the image can run.
    #[test]
    fn a_mode_without_fallback_has_no_reaction() {
        let hazard = HAZARD.replace("/handler/stop_path", "demo.launch.xml/safe_stop");
        let yaml = model_yaml(AGE_100, "", &format!("{hazard}      ftti_ms: 500.0\n"), "").replace(
            "\n  hazards:",
            "\n  modes:\n    demo.launch.xml/safe_stop:\n      \
                 reaction: /handler/stop_path\n  hazards:",
        );
        let m = parse(&yaml);
        assert!(hazard_budgets(&m).expect("resolves")[0].reaction.is_none());
    }

    /// A ladder as the resolver writes it: the hazard enters `driving`,
    /// whose fallback is `hold` (a dwelling level with its own `within`),
    /// `slow` and `safe_stop`, the minimal risk condition. `levels` is the
    /// model's `degradation_levels` (the levels the hazard runs, each with
    /// the resolver's route); `within` is `hold`'s deadline.
    fn ladder_model(levels: &str, ftti: &str, within: &str) -> SystemModel {
        let hazard = HAZARD.replace(
            "reaction: /handler/stop_path",
            "reaction: demo.launch.xml/driving",
        );
        let paths = "    /handler/hold_path:\n      input: [/handler/scan]\n      \
                     output: [/handler/stop]\n      max_latency_ms: 5.0\n      \
                     safe_state:\n        emits: /handler/stop\n        settle_ms: 50.0\n    \
                     /handler/slow_path:\n      input: [/handler/scan]\n      \
                     output: [/handler/stop]\n      max_latency_ms: 5.0\n";
        let yaml = model_yaml(
            AGE_100,
            paths,
            &format!("{hazard}      ftti_ms: {ftti}\n      degradation_levels:\n{levels}"),
            "",
        )
        .replace(
            "\n  hazards:",
            &format!(
                "\n  modes:\n    demo.launch.xml/driving:\n      \
                 fallback: [demo.launch.xml/hold, demo.launch.xml/slow, \
                 demo.launch.xml/safe_stop]\n    \
                 demo.launch.xml/hold:\n      reaction: /handler/hold_path\n      \
                 min_dwell:\n        duration_ms: 2000.0\n        max_ms: 5000.0\n      \
                 within_ms: {within}\n    \
                 demo.launch.xml/slow:\n      reaction: /handler/slow_path\n    \
                 demo.launch.xml/safe_stop:\n      reaction: /handler/stop_path\n  hazards:"
            ),
        );
        parse(&yaml)
    }

    const ALL_LEVELS: &str = "        - { level: hold, role: dwell, route_ms: 12.0 }\n        \
                              - { level: slow, role: level, route_ms: 17.0 }\n        \
                              - { level: safe_stop, role: mrc, route_ms: 30.0 }\n";

    /// phase-560 W8 (#3758) -- every level the hazard runs has a row:
    /// detection + the dwelling levels waited out above it (route + charged
    /// dwell, a range's `max`) + its route + its settle. The dwelling level
    /// is judged against its `within` only, the others against the `ftti`.
    #[test]
    fn every_level_of_the_ladder_has_a_row() {
        let m = ladder_model(ALL_LEVELS, "10000.0", "200.0");
        let b = &check_model(&m).expect("every pair fits")[0];
        let rows: Vec<(&str, LevelRole, Option<f64>)> = b
            .levels
            .iter()
            .map(|l| (l.level.as_str(), l.role, l.total_ms))
            .collect();
        // hold: 100 + 12 + 50 (its safe state's settle); slow: 100 +
        // (12 + 5000) + 17; safe_stop: 100 + 5012 + 30.
        assert_eq!(
            rows,
            [
                ("hold", LevelRole::Dwell, Some(162.0)),
                ("slow", LevelRole::Level, Some(5129.0)),
                ("safe_stop", LevelRole::Mrc, Some(5142.0)),
            ]
        );
        assert_eq!(b.levels[0].route, Some((12.0, RouteSource::Model)));
        let judged: Vec<(usize, Deadline)> =
            b.judgements().iter().map(|j| (j.row, j.deadline)).collect();
        assert_eq!(
            judged,
            [
                (0, Deadline::Within),
                (1, Deadline::Ftti),
                (2, Deadline::Ftti)
            ]
        );
        // hold binds: 162 of 200 is 81 %, more than 5142 of 10 s.
        let j = b.binding().expect("a binding pair");
        assert_eq!((j.row, j.deadline), (0, Deadline::Within));
        assert_eq!(
            b.total_ms(),
            Some(5142.0),
            "the minimal risk condition's row"
        );
    }

    /// A level above the minimal risk condition that misses the `ftti`
    /// refuses. The old budget charged only the minimal risk condition
    /// (5142 ms, which fits), so it built this image.
    #[test]
    fn a_level_above_the_floor_that_misses_the_ftti_refuses() {
        let levels = ALL_LEVELS.replace("route_ms: 17.0", "route_ms: 60.0");
        let m = ladder_model(&levels, "5150.0", "200.0");
        let b = &hazard_budgets(&m).expect("resolves")[0];
        assert!(b.total_ms().unwrap() <= 5150.0, "the floor alone fits");
        let err = check_model(&m).expect_err("slow (5172 ms) misses 5150 ms");
        let text = err.to_string();
        for needle in [
            "binding level `slow` (level)",
            "level `slow` (level) 5172.0 ms, MISSES ftti by 22.0 ms",
            "/handler/slow_path",
        ] {
            assert!(text.contains(needle), "names `{needle}`: {text}");
        }
    }

    /// A dwelling level's own `within` (rlm v0.3.18 `modes.<m>.within`) is
    /// judged: 162 ms against 70 ms, missed by 92 ms.
    #[test]
    fn a_missed_within_refuses() {
        let m = ladder_model(ALL_LEVELS, "90000.0", "70.0");
        let err = check_model(&m).expect_err("hold: 162 ms > within 70 ms");
        let FttiBudgetError::Exceeded(bs) = &err else {
            panic!("{err}");
        };
        let misses: Vec<(usize, Deadline)> =
            bs[0].misses().iter().map(|j| (j.row, j.deadline)).collect();
        assert_eq!(misses, [(0, Deadline::Within)]);
        assert!(
            err.to_string().contains(
                "level `hold` (dwell) 162.0 ms, within 70.0 ms, MISSES within by 92.0 ms"
            ),
            "{err}"
        );
    }

    /// A level the hazard skips (rlm v0.3.18 step 2) is absent from the
    /// model's `degradation_levels`: it is neither judged nor charged, so its
    /// dwell is not waited out above the levels below.
    #[test]
    fn a_skipped_level_is_not_charged() {
        let levels = "        - { level: slow, role: level, route_ms: 17.0 }\n        \
                      - { level: safe_stop, role: mrc, route_ms: 30.0 }\n";
        let m = ladder_model(levels, "10000.0", "70.0");
        let b = &check_model(&m).expect("the 70 ms level is skipped")[0];
        let totals: Vec<(&str, Option<f64>)> = b
            .levels
            .iter()
            .map(|l| (l.level.as_str(), l.total_ms))
            .collect();
        assert_eq!(totals, [("slow", Some(117.0)), ("safe_stop", Some(130.0))]);
    }

    /// A model level that is no mode is refused, not guessed.
    #[test]
    fn an_unknown_level_refuses() {
        let levels = ALL_LEVELS.replace("level: slow,", "level: nowhere,");
        let err =
            hazard_budgets(&ladder_model(&levels, "10000.0", "70.0")).expect_err("must refuse");
        assert!(err.to_string().contains("`nowhere`"), "{err}");
    }

    /// A model with no hazards is untouched.
    #[test]
    fn no_hazards_no_budgets() {
        let m = parse(&model_yaml(AGE_100, "", "      {}\n", "").replace(
            "\n  hazards:\n    demo.launch.xml/scan_lost:\n      {}\n",
            "\n",
        ));
        assert!(check_model(&m).expect("nothing to check").is_empty());
    }
}
