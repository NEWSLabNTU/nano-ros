//! phase-462 W3a -- a hazard's fault-tolerant time interval (FTTI) is checked
//! against what the IMAGE will run, at build time.
//!
//! rlm checks `ftti` against the contract's own declared latencies. This is
//! the other half: the bake knows how fast the executor can SEE a fault (the
//! monitor rows it bakes), how long the reaction path may take (its
//! `max_latency_ms`) and how long that path may wait to be released (its timer
//! period, or the period of the tier that runs it). When those three add up to
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
//! - `contracts.hazards.<scope>/<name>`: `guards` (each one fault: topic
//!   FQNs, faulting when ANY member does, or every member when `all_of`),
//!   `on` (the fault classes; empty means every class), `ftti_ms`, and
//!   `reaction` -- a key into `contracts.modes` (whose own `reaction` is a
//!   path key), else a path key directly.
//! - the reaction path is a `contracts.scope_paths` entry, or a
//!   `contracts.node_paths` one.
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
//! The fastest applicable row wins for each member. `overrun` and `reported`
//! have no executor-side detector on a guard topic (a `reported` fault is the
//! application's own diagnostic), so a class with no row is UNOBSERVED here:
//! it is listed on the hazard's budget and never counted -- the Linux side,
//! or the application, may observe it, and a check that guessed would refuse
//! a correct system or pass a broken one.
//!
//! # When this abstains
//!
//! A hazard with no `ftti_ms`, no observed detector, or a reaction with no
//! `max_latency_ms` has nothing to add up, and is reported as such rather than
//! refused: the refusal is reserved for a budget that is PROVABLY exceeded.

use std::{collections::BTreeMap, fmt};

use ros_launch_manifest_model::{FaultKind, HazardContract, PathContract, SystemModel};
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

/// The reaction term of a budget.
#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    /// The path the hazard's reaction resolves to.
    pub path: String,
    /// The mode it was reached through, when the hazard enters one.
    pub via_mode: Option<String>,
    /// The path's `max_latency_ms`; `None` when it states none.
    pub max_latency_ms: Option<f64>,
    /// How long the path may wait to be released, ms, and what says so.
    pub release: Option<(f64, String)>,
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
    /// The reaction, when the hazard names one.
    pub reaction: Option<Reaction>,
}

impl HazardBudget {
    /// Detection + reaction + release, ms -- `None` when any term is unknown.
    pub fn total_ms(&self) -> Option<f64> {
        let det = self.detection.as_ref()?.bound_ms;
        let r = self.reaction.as_ref()?;
        let lat = r.max_latency_ms?;
        Some(det + lat + r.release.as_ref().map_or(0.0, |(ms, _)| *ms))
    }

    /// True when the budget is provably exceeded.
    pub fn exceeds_ftti(&self) -> bool {
        match (self.ftti_ms, self.total_ms()) {
            (Some(ftti), Some(total)) => total > ftti + 1e-9,
            _ => false,
        }
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
        if let Some(total) = self.total_ms() {
            write!(f, ", the image needs {}", ms(total))?;
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
        match &self.reaction {
            Some(r) => {
                let via = r
                    .via_mode
                    .as_ref()
                    .map(|m| format!(" (through mode `{m}`)"))
                    .unwrap_or_default();
                match r.max_latency_ms {
                    Some(lat) => writeln!(
                        f,
                        "  reaction {}: path `{}`{via} max_latency_ms",
                        ms(lat),
                        r.path
                    )?,
                    None => writeln!(
                        f,
                        "  reaction: path `{}`{via} states no max_latency_ms",
                        r.path
                    )?,
                }
                if let Some((wait, why)) = &r.release {
                    writeln!(f, "  release {}: {why}", ms(*wait))?;
                }
            }
            None => writeln!(f, "  reaction: none named")?,
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
    /// The mode a hazard enters names a reaction that is no path.
    UnknownModeReaction {
        hazard: String,
        mode: String,
        reaction: String,
    },
    /// One or more hazards whose image cannot meet their FTTI.
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
                reaction,
            } => write!(
                f,
                "contract: hazard `{hazard}` enters mode `{mode}`, whose `reaction` names \
                 `{reaction}` -- not a path in this model."
            ),
            FttiBudgetError::Exceeded(bs) => {
                writeln!(
                    f,
                    "{} hazard(s) cannot meet their fault-tolerant time interval on this image: \
                     detecting the fault, running the reaction path and waiting for its release \
                     take longer than `ftti_ms`. Shorten a term (a tighter `max_age_ms` on the \
                     guard, a faster reaction, a shorter period) or relax the FTTI.",
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

/// The classes a hazard reports; an empty `on` means every class.
fn classes(h: &HazardContract) -> Vec<FaultKind> {
    if h.on.is_empty() {
        vec![
            FaultKind::Omission,
            FaultKind::Late,
            FaultKind::Loss,
            FaultKind::Reported,
            FaultKind::Overrun,
        ]
    } else {
        h.on.clone()
    }
}

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
        FaultKind::Reported | FaultKind::Overrun => None,
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
/// stated jitter), else the period of the tier that runs its first input's
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
    let tier = model
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
    let ms = model.execution.tiers.get(tier)?.period?.as_millis_f64();
    Some((
        ms,
        format!("tier `{tier}`'s period, which releases node `{node}`"),
    ))
}

fn path<'m>(model: &'m SystemModel, key: &str) -> Option<&'m PathContract> {
    model
        .contracts
        .scope_paths
        .get(key)
        .or_else(|| model.contracts.node_paths.get(key))
}

fn reaction(
    model: &SystemModel,
    hazard: &str,
    h: &HazardContract,
) -> Result<Option<Reaction>, FttiBudgetError> {
    let Some(r) = h.reaction.as_deref() else {
        return Ok(None);
    };
    // A mode first, the way the checker resolves it (the model's own rule).
    let (key, via_mode) = match model.contracts.modes.get(r) {
        Some(mode) => match mode.reaction.as_deref() {
            Some(k) => (k, Some(r.to_string())),
            // A mode with no reaction path: entering it is the reaction, and
            // there is no latency to add.
            None => return Ok(None),
        },
        None => (r, None),
    };
    let Some(p) = path(model, key) else {
        return Err(match via_mode {
            Some(mode) => FttiBudgetError::UnknownModeReaction {
                hazard: hazard.to_string(),
                mode,
                reaction: key.to_string(),
            },
            None => FttiBudgetError::UnknownReaction {
                hazard: hazard.to_string(),
                reaction: key.to_string(),
            },
        });
    };
    Ok(Some(Reaction {
        path: key.to_string(),
        via_mode,
        max_latency_ms: p.max_latency_ms,
        release: release(model, p),
    }))
}

/// Every hazard's budget, as this image runs it. Refuses only a reaction the
/// model cannot resolve; an exceeded budget is [`check_model`]'s verdict.
pub fn hazard_budgets(model: &SystemModel) -> Result<Vec<HazardBudget>, FttiBudgetError> {
    let mut out = Vec::new();
    for (key, h) in &model.contracts.hazards {
        let mut detection: Option<Detector> = None;
        let mut unobserved = Vec::new();
        for kind in classes(h) {
            for guard in &h.guards {
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
        out.push(HazardBudget {
            hazard: key.clone(),
            ftti_ms: h.ftti_ms,
            detection,
            unobserved,
            reaction: reaction(model, key, h)?,
        });
    }
    Ok(out)
}

/// The build-time verdict: every hazard whose budget this image provably
/// exceeds, as one refusal naming each term.
pub fn check_model(model: &SystemModel) -> Result<Vec<HazardBudget>, FttiBudgetError> {
    let budgets = hazard_budgets(model)?;
    let over: Vec<HazardBudget> = budgets
        .iter()
        .filter(|b| b.exceeds_ftti())
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
mod tests {
    use super::*;

    /// A sensor publishing `/scan`, a handler subscribing it, and a hazard on
    /// `/scan` whose reaction is the handler's stop path. `extra` lands under
    /// `contracts:`; `hazard` is the hazard body; `tiers` goes under
    /// `execution:`.
    fn model_yaml(sub_age: &str, path_extra: &str, hazard: &str, tiers: &str) -> String {
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
             \nexecution:\n  bindings:\n    /handler: rt\n  tiers:\n    rt:\n      \
             class: real_time\n{tiers}"
        )
    }

    fn parse(yaml: &str) -> SystemModel {
        SystemModel::from_yaml_str(yaml).expect("fixture parses")
    }

    const AGE_100: &str = "      max_age_ms: 100.0\n";
    const HAZARD: &str = "      guards:\n        - members: [/scan]\n      on: omission\n      \
                          reaction: /handler/stop_path\n";

    fn hazard_with_ftti(ftti: &str) -> String {
        format!("{HAZARD}      ftti_ms: {ftti}\n")
    }

    /// The terms add up the way the module says: silence on the guard (100)
    /// + the reaction path (20) + its tier's period (10).
    #[test]
    fn a_budget_is_detection_plus_reaction_plus_release() {
        let m = parse(&model_yaml(
            AGE_100,
            "",
            &hazard_with_ftti("500.0"),
            "      period_us: 10000\n",
        ));
        let b = &hazard_budgets(&m).expect("resolves")[0];
        let d = b.detection.as_ref().expect("observed");
        assert_eq!((d.rule, d.bound_ms), ("silence-runtime", 100.0));
        assert_eq!(b.total_ms(), Some(130.0));
        assert!(!b.exceeds_ftti());
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
            "      period_us: 10000\n",
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
            "tier `rt`",
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

    /// A hazard that enters a mode reacts through the mode's own path.
    #[test]
    fn a_mode_reaction_resolves_through_the_mode() {
        let hazard = HAZARD.replace("/handler/stop_path", "demo.launch.xml/safe_stop");
        let yaml = model_yaml(AGE_100, "", &format!("{hazard}      ftti_ms: 500.0\n"), "").replace(
            "\n  hazards:",
            "\n  modes:\n    demo.launch.xml/safe_stop:\n      \
                 reaction: /handler/stop_path\n  hazards:",
        );
        let m = parse(&yaml);
        let b = &hazard_budgets(&m).expect("resolves")[0];
        let r = b.reaction.as_ref().expect("reaction");
        assert_eq!(r.path, "/handler/stop_path");
        assert_eq!(r.via_mode.as_deref(), Some("demo.launch.xml/safe_stop"));
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
