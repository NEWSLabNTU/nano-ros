//! phase-509 P4 (RFC-0108 D1) -- the task record: one per callback, derived
//! from the contract.
//!
//! Every callback the model can see is one dispatchable slot, and each gets a
//! record of how it is RELEASED:
//!
//! | release kind | parameters | contract source |
//! | --- | --- | --- |
//! | periodic | T; D = T unless stated | a timer path's `rate_hz`; its `max_latency` |
//! | sporadic | T_min; D | `min_interarrival` on the input, or a rate (`max_rate_hz`, the topic's `rate_hz`) |
//! | served | Q per P; D | `budget` + `budget_period` on the input |
//! | unanalysable | none | an event input with none of these |
//!
//! C (execution time) is the RFC-0078 profile's bound at the path boundary
//! (`"<node>/<path>"`), when the caller has a profile; absent otherwise, and
//! never invented ([`crate::wcet`]).
//!
//! # What counts as a callback
//!
//! What the model names, the same set [`crate::executor_sizing`] counts plus
//! the timers it cannot see there:
//!
//! - a timer: a node path whose trigger is `Timer` (the model has no timer
//!   entity; the path IS the timer);
//! - a subscription and a service server: an INPUT, the place the new keys
//!   go (rlm v0.1.68);
//! - a service client: its response callback. A response follows the node's
//!   own request, so it inherits the release of the paths that issue the
//!   request (`output: [<client>]`): the least T among them, each issuing path
//!   released by its timer or by its inputs. rlm refuses the keys on `cli:`
//!   for that reason;
//! - an action server or client: the contract has no release key for an
//!   action endpoint, so its callbacks are unanalysable.
//!
//! A subscriber's `min_rate_hz` is a FLOOR (what it needs), not a bound on
//! its arrivals, so it releases nothing here.
//!
//! # The rule (RFC-0108 decision 4)
//!
//! An unanalysable callback is REFUSED on a safety image, an image whose
//! contract supervises a deadline (a node path carrying `supervise:`,
//! [`crate::derive::supervised_nodes`]): [`check_model`] fails, naming each
//! callback and the two keys that would fix it. On any other image it is
//! REPORTED: [`unanalysable_degradations`], which the derived schedule
//! carries in its degradation record.

use std::collections::{BTreeMap, BTreeSet};

use ros_launch_manifest_model::SystemModel;
use ros_launch_manifest_sched::EffectiveTrigger;
use serde::Serialize;

use crate::{rtos_realizer::Degradation, wcet::WcetProfile};

/// How a callback is released (RFC-0108 D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseKind {
    /// A timer: period T.
    Periodic,
    /// An input with a least inter-arrival time T_min.
    Sporadic,
    /// An input with a budget Q per server period P: analysed as a periodic
    /// task (Q, P) behind a server.
    Served,
    /// An event input with nothing that bounds its arrivals.
    Unanalysable,
}

impl ReleaseKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ReleaseKind::Periodic => "periodic",
            ReleaseKind::Sporadic => "sporadic",
            ReleaseKind::Served => "served",
            ReleaseKind::Unanalysable => "unanalysable",
        }
    }
}

/// What the callback is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackKind {
    Timer,
    Subscription,
    ServiceServer,
    /// A service client's response callback.
    ServiceClient,
    ActionServer,
    ActionClient,
}

impl CallbackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CallbackKind::Timer => "timer",
            CallbackKind::Subscription => "subscription",
            CallbackKind::ServiceServer => "service server",
            CallbackKind::ServiceClient => "service client",
            CallbackKind::ActionServer => "action server",
            CallbackKind::ActionClient => "action client",
        }
    }
}

/// Where the release parameter came from: the contract fact a reviewer reads
/// to check the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseSource {
    /// A timer path's `rate_hz`.
    TimerRate,
    /// The input's `min_interarrival`.
    MinInterarrival,
    /// The subscriber's `max_rate_hz`.
    MaxRate,
    /// The subscribed topic's `rate_hz`.
    TopicRate,
    /// The input's `budget` + `budget_period`.
    Budget,
    /// A client response: the release of the paths that issue the request.
    Request,
    /// Nothing: the callback is unanalysable.
    None,
}

/// One callback's task record (RFC-0108 D1). Times in microseconds.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TaskRecord {
    /// `"<node FQN>/<path>"` for a timer, the endpoint ref
    /// `"<node FQN>/<endpoint>"` for every other callback.
    pub callback: String,
    /// The node FQN.
    pub node: String,
    pub kind: CallbackKind,
    pub release: ReleaseKind,
    pub source: ReleaseSource,
    /// T (periodic) or T_min (sporadic). `None` for served and unanalysable.
    pub period_us: Option<u64>,
    /// D: the least `max_latency` of the paths the callback runs (and a
    /// server's `max_response`), else T, T_min or P (an implicit deadline).
    /// `None` when unanalysable and nothing states one.
    pub deadline_us: Option<u64>,
    /// Whether D was stated rather than implicit.
    pub deadline_stated: bool,
    /// C: the summed RFC-0078 bounds of the paths the callback runs, when
    /// every one of them declares one.
    pub exec_us: Option<u64>,
    /// Q (served only).
    pub budget_us: Option<u64>,
    /// P (served only).
    pub budget_period_us: Option<u64>,
}

fn us(ms: f64) -> u64 {
    (ms * 1000.0).round().max(0.0) as u64
}

fn node_of(ep: &str) -> &str {
    ep.rsplit_once('/').map(|(n, _)| n).unwrap_or(ep)
}

/// A release, before the deadline and the cost are attached.
#[derive(Clone, Copy)]
struct Release {
    kind: ReleaseKind,
    source: ReleaseSource,
    period_ms: Option<f64>,
    budget_ms: Option<f64>,
    budget_period_ms: Option<f64>,
}

impl Release {
    const NONE: Release = Release {
        kind: ReleaseKind::Unanalysable,
        source: ReleaseSource::None,
        period_ms: None,
        budget_ms: None,
        budget_period_ms: None,
    };

    fn sporadic(source: ReleaseSource, t_ms: f64) -> Release {
        Release {
            kind: ReleaseKind::Sporadic,
            source,
            period_ms: Some(t_ms),
            ..Release::NONE
        }
    }

    /// The least time between two releases, for a client that inherits it:
    /// T, T_min, or a server's period (it releases at most once per P when
    /// its budget covers one callback; a looser reading would be unsafe the
    /// other way, so a served input passes its P on).
    fn spacing_ms(&self) -> Option<f64> {
        match self.kind {
            ReleaseKind::Periodic | ReleaseKind::Sporadic => self.period_ms,
            ReleaseKind::Served => self.budget_period_ms,
            ReleaseKind::Unanalysable => None,
        }
    }
}

/// An input's release from its contract: a budget first (it decides the
/// kind), then `min_interarrival`, then a rate bound.
fn input_release(
    min_interarrival_ms: Option<f64>,
    budget: Option<(f64, f64)>,
    rate_hz: Option<(ReleaseSource, f64)>,
) -> Release {
    if let Some((q, p)) = budget {
        return Release {
            kind: ReleaseKind::Served,
            source: ReleaseSource::Budget,
            period_ms: None,
            budget_ms: Some(q),
            budget_period_ms: Some(p),
        };
    }
    if let Some(t) = min_interarrival_ms.filter(|t| *t > 0.0) {
        return Release::sporadic(ReleaseSource::MinInterarrival, t);
    }
    if let Some((source, hz)) = rate_hz.filter(|(_, hz)| hz.is_finite() && *hz > 0.0) {
        return Release::sporadic(source, 1000.0 / hz);
    }
    Release::NONE
}

/// The task record of every callback the model names, sorted by callback.
/// `wcet` supplies C where it declares a bound; `None` leaves C absent.
pub fn task_records(model: &SystemModel, wcet: Option<&WcetProfile>) -> Vec<TaskRecord> {
    let c = &model.contracts;
    let exec_ms = |boundary: &str| wcet.and_then(|w| w.exec_ms(boundary));

    // Input releases (subscriptions and servers), keyed by endpoint ref.
    let mut inputs: BTreeMap<String, (CallbackKind, Release)> = BTreeMap::new();
    for (topic, w) in &model.structure.topics {
        let topic_rate = c.topics.get(topic).and_then(|t| t.rate_hz);
        for ep in &w.subscribers {
            let sc = c.sub_endpoints.get(ep);
            let rate = sc
                .and_then(|s| s.max_rate_hz)
                .map(|hz| (ReleaseSource::MaxRate, hz))
                .or(topic_rate.map(|hz| (ReleaseSource::TopicRate, hz)));
            let release = input_release(
                sc.and_then(|s| s.min_interarrival_ms),
                sc.and_then(|s| s.budget_ms.zip(s.budget_period_ms)),
                rate,
            );
            inputs.insert(ep.clone(), (CallbackKind::Subscription, release));
        }
    }
    for w in model.structure.services.values() {
        for ep in &w.server {
            let sc = c.srv_endpoints.get(ep);
            let release = input_release(
                sc.and_then(|s| s.min_interarrival_ms),
                sc.and_then(|s| s.budget_ms.zip(s.budget_period_ms)),
                None,
            );
            inputs.insert(ep.clone(), (CallbackKind::ServiceServer, release));
        }
    }

    // A path's release: its timer, or the most frequent of its inputs (any
    // of them fires it). `None` when any input it reads is unbounded.
    let path_spacing_ms = |p: &ros_launch_manifest_model::PathContract| -> Option<f64> {
        match p.effective_trigger() {
            EffectiveTrigger::Timer { rate_hz } if rate_hz > 0.0 => Some(1000.0 / rate_hz),
            EffectiveTrigger::Input(_) => {
                let mut least: Option<f64> = None;
                for ep in &p.input {
                    let t = inputs.get(ep).and_then(|(_, r)| r.spacing_ms())?;
                    least = Some(least.map_or(t, |l: f64| l.min(t)));
                }
                least
            }
            _ => None,
        }
    };

    // The paths each endpoint feeds (as an input) or is issued by (as an
    // output), with their boundary key.
    let mut feeds: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut issued_by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (key, p) in &c.node_paths {
        for ep in &p.input {
            feeds.entry(ep.as_str()).or_default().push(key.as_str());
        }
        for ep in &p.output {
            issued_by.entry(ep.as_str()).or_default().push(key.as_str());
        }
    }
    // D and C of a callback that runs `paths`.
    let stated_deadline_ms = |paths: &[&str]| -> Option<f64> {
        paths
            .iter()
            .filter_map(|k| c.node_paths.get(*k).and_then(|p| p.max_latency_ms))
            .reduce(f64::min)
    };
    let summed_exec_ms = |paths: &[&str]| -> Option<f64> {
        if paths.is_empty() {
            return None;
        }
        paths.iter().map(|k| exec_ms(k)).sum::<Option<f64>>()
    };

    let mut out: Vec<TaskRecord> = Vec::new();
    let mut push = |callback: String,
                    kind: CallbackKind,
                    r: Release,
                    stated_d: Option<f64>,
                    exec: Option<f64>| {
        let implicit = match r.kind {
            ReleaseKind::Periodic | ReleaseKind::Sporadic => r.period_ms,
            ReleaseKind::Served => r.budget_period_ms,
            ReleaseKind::Unanalysable => None,
        };
        out.push(TaskRecord {
            node: node_of(&callback).to_string(),
            callback,
            kind,
            release: r.kind,
            source: r.source,
            period_us: r.period_ms.map(us),
            deadline_us: stated_d.or(implicit).map(us),
            deadline_stated: stated_d.is_some(),
            exec_us: exec.map(us),
            budget_us: r.budget_ms.map(us),
            budget_period_us: r.budget_period_ms.map(us),
        });
    };

    // Timers.
    for (key, p) in &c.node_paths {
        if let EffectiveTrigger::Timer { rate_hz } = p.effective_trigger()
            && rate_hz > 0.0
        {
            let r = Release {
                kind: ReleaseKind::Periodic,
                source: ReleaseSource::TimerRate,
                period_ms: Some(1000.0 / rate_hz),
                ..Release::NONE
            };
            push(
                key.clone(),
                CallbackKind::Timer,
                r,
                p.max_latency_ms,
                exec_ms(key),
            );
        }
    }
    // Subscriptions and servers.
    for (ep, (kind, r)) in &inputs {
        let paths: &[&str] = feeds.get(ep.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        let mut d = stated_deadline_ms(paths);
        if *kind == CallbackKind::ServiceServer
            && let Some(resp) = c.srv_endpoints.get(ep).and_then(|s| s.max_response_ms)
        {
            d = Some(d.map_or(resp, |d| d.min(resp)));
        }
        push(ep.clone(), *kind, *r, d, summed_exec_ms(paths));
    }
    // Service clients: the response inherits the issuing paths' release.
    for w in model.structure.services.values() {
        for ep in &w.client {
            let issuers: &[&str] = issued_by.get(ep.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            let spacing = if issuers.is_empty() {
                None
            } else {
                issuers
                    .iter()
                    .map(|k| c.node_paths.get(*k).and_then(path_spacing_ms))
                    .collect::<Option<Vec<f64>>>()
                    .and_then(|ts| ts.into_iter().reduce(f64::min))
            };
            let r = spacing.map_or(Release::NONE, |t| {
                Release::sporadic(ReleaseSource::Request, t)
            });
            let paths: &[&str] = feeds.get(ep.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            push(
                ep.clone(),
                CallbackKind::ServiceClient,
                r,
                stated_deadline_ms(paths),
                summed_exec_ms(paths),
            );
        }
    }
    // Actions: no release key in the grammar.
    for w in model.structure.actions.values() {
        for (eps, kind) in [
            (&w.server, CallbackKind::ActionServer),
            (&w.client, CallbackKind::ActionClient),
        ] {
            for ep in eps {
                push(ep.clone(), kind, Release::NONE, None, None);
            }
        }
    }
    out.sort_by(|a, b| a.callback.cmp(&b.callback));
    out
}

/// What would fix one unanalysable callback, for its report and refusal.
fn fix_for(t: &TaskRecord) -> String {
    let ep = t
        .callback
        .rsplit_once('/')
        .map(|(_, e)| e)
        .unwrap_or(&t.callback);
    match t.kind {
        CallbackKind::Subscription => format!(
            "declare `min_interarrival: <duration>` or `budget: <duration>` + \
             `budget_period: <duration>` on `nodes.<{}>.sub.{ep}` (or a rate bound, \
             `max_rate_hz` or the topic's `rate_hz`)",
            t.node
        ),
        CallbackKind::ServiceServer => format!(
            "declare `min_interarrival: <duration>` or `budget: <duration>` + \
             `budget_period: <duration>` on `nodes.<{}>.srv.{ep}`",
            t.node
        ),
        CallbackKind::ServiceClient => format!(
            "its responses follow its requests: bound the paths that issue them \
             (`output: [{ep}]`), each released by a timer or by inputs that declare \
             `min_interarrival` or `budget` + `budget_period`"
        ),
        CallbackKind::ActionServer | CallbackKind::ActionClient => {
            "the contract grammar has no release bound for an action endpoint yet".to_string()
        }
        CallbackKind::Timer => unreachable!("a timer is always periodic"),
    }
}

/// One `unanalysable` degradation per unanalysable callback (RFC-0108 D1):
/// what an image WITHOUT `supervise:` reports.
pub fn unanalysable_degradations(records: &[TaskRecord]) -> Vec<Degradation> {
    records
        .iter()
        .filter(|t| t.release == ReleaseKind::Unanalysable)
        .map(|t| Degradation {
            node: t.callback.clone(),
            dim: "unanalysable",
            reason: format!(
                "this {} has nothing that bounds its arrivals, so no response-time \
                 analysis can bound the interference it adds; it is placed best-effort. \
                 To make it analysable, {}",
                t.kind.as_str(),
                fix_for(t)
            ),
        })
        .collect()
}

/// The refusal [`check_model`] returns on a safety image.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{}", render_refusal(supervised, unanalysable))]
pub struct UnanalysableOnSafetyImage {
    /// The node FQNs whose contract supervises a deadline.
    pub supervised: BTreeSet<String>,
    /// Every unanalysable callback, by record.
    pub unanalysable: Vec<TaskRecord>,
}

fn render_refusal(supervised: &BTreeSet<String>, unanalysable: &[TaskRecord]) -> String {
    let mut s = format!(
        "RFC-0108 D1: this image supervises a deadline (`supervise:` on {}), and {} \
         callback(s) have nothing that bounds their arrivals. An input with unbounded \
         arrivals adds interference no analysis can bound, which can break the \
         supervised reaction. Each one:",
        supervised
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", "),
        unanalysable.len()
    );
    for t in unanalysable {
        s.push_str(&format!(
            "\n  - `{}` ({}): {}",
            t.callback,
            t.kind.as_str(),
            fix_for(t)
        ));
    }
    s
}

/// RFC-0108 decision 4: refuse an unanalysable callback on a safety image.
///
/// `Ok` with the records on every image that supervises nothing, or whose
/// callbacks are all analysable. The report for the first case is
/// [`unanalysable_degradations`]; the derived schedule carries it.
pub fn check_model(model: &SystemModel) -> Result<Vec<TaskRecord>, UnanalysableOnSafetyImage> {
    let records = task_records(model, None);
    let supervised = crate::derive::supervised_nodes(model);
    let unanalysable: Vec<TaskRecord> = records
        .iter()
        .filter(|t| t.release == ReleaseKind::Unanalysable)
        .cloned()
        .collect();
    if supervised.is_empty() || unanalysable.is_empty() {
        return Ok(records);
    }
    Err(UnanalysableOnSafetyImage {
        supervised,
        unanalysable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ros_launch_manifest_model::{
        Contracts, PathContract, ServiceWiring, SrvContract, Structure, SubContract,
        SuperviseContract, SuperviseMechanism, TopicContract, TopicWiring,
    };

    fn s(v: &str) -> String {
        v.to_string()
    }

    /// One node `/n` with a 10 Hz timer path, a subscription per release
    /// kind, a server, and a client its timer path issues requests on.
    fn fixture() -> SystemModel {
        let mut m = SystemModel::default();
        let st: &mut Structure = &mut m.structure;
        for (topic, sub) in [
            ("/t_rate", "/n/by_topic_rate"),
            ("/t_gap", "/n/by_gap"),
            ("/t_budget", "/n/by_budget"),
            ("/t_none", "/n/unbounded"),
            ("/t_cap", "/n/by_max_rate"),
        ] {
            st.topics.insert(
                s(topic),
                TopicWiring {
                    msg_type: s("std_msgs/msg/Empty"),
                    subscribers: vec![s(sub)],
                    ..Default::default()
                },
            );
        }
        st.services.insert(
            s("/svc"),
            ServiceWiring {
                srv_type: s("std_srvs/srv/Trigger"),
                server: vec![s("/n/serve")],
                client: vec![s("/n/ask")],
            },
        );
        let c: &mut Contracts = &mut m.contracts;
        c.topics.insert(
            s("/t_rate"),
            TopicContract {
                rate_hz: Some(20.0),
                ..Default::default()
            },
        );
        c.sub_endpoints.insert(
            s("/n/by_gap"),
            SubContract {
                min_rate_hz: Some(10.0),
                min_interarrival_ms: Some(50.0),
                ..Default::default()
            },
        );
        c.sub_endpoints.insert(
            s("/n/by_budget"),
            SubContract {
                budget_ms: Some(2.0),
                budget_period_ms: Some(100.0),
                ..Default::default()
            },
        );
        c.sub_endpoints.insert(
            s("/n/unbounded"),
            SubContract {
                // A floor is what the subscriber needs; it bounds nothing.
                min_rate_hz: Some(10.0),
                ..Default::default()
            },
        );
        c.sub_endpoints.insert(
            s("/n/by_max_rate"),
            SubContract {
                max_rate_hz: Some(4.0),
                ..Default::default()
            },
        );
        c.srv_endpoints.insert(
            s("/n/serve"),
            SrvContract {
                max_response_ms: Some(5.0),
                min_interarrival_ms: Some(200.0),
                ..Default::default()
            },
        );
        c.node_paths.insert(
            s("/n/tick"),
            PathContract {
                trigger: Some(EffectiveTrigger::Timer { rate_hz: 10.0 }),
                output: vec![s("/n/ask")],
                ..Default::default()
            },
        );
        c.node_paths.insert(
            s("/n/react"),
            PathContract {
                trigger: Some(EffectiveTrigger::Input(vec![s("gap")])),
                input: vec![s("/n/by_gap")],
                output: vec![s("/n/out")],
                max_latency_ms: Some(30.0),
                ..Default::default()
            },
        );
        m
    }

    fn get<'a>(r: &'a [TaskRecord], cb: &str) -> &'a TaskRecord {
        r.iter()
            .find(|t| t.callback == cb)
            .unwrap_or_else(|| panic!("no record for {cb}: {r:#?}"))
    }

    /// Acceptance: each release kind is derived from a fixture contract.
    #[test]
    fn each_release_kind_is_derived_from_the_contract() {
        let r = task_records(&fixture(), None);
        assert_eq!(r.len(), 8, "{r:#?}");

        let tick = get(&r, "/n/tick");
        assert_eq!(
            (tick.kind, tick.release, tick.source),
            (
                CallbackKind::Timer,
                ReleaseKind::Periodic,
                ReleaseSource::TimerRate
            )
        );
        assert_eq!(tick.period_us, Some(100_000));
        assert_eq!(
            (tick.deadline_us, tick.deadline_stated),
            (Some(100_000), false)
        );

        let gap = get(&r, "/n/by_gap");
        assert_eq!(
            (gap.release, gap.source),
            (ReleaseKind::Sporadic, ReleaseSource::MinInterarrival)
        );
        assert_eq!(gap.period_us, Some(50_000));
        // D is the path it runs, stated.
        assert_eq!((gap.deadline_us, gap.deadline_stated), (Some(30_000), true));

        let rate = get(&r, "/n/by_topic_rate");
        assert_eq!(
            (rate.release, rate.source, rate.period_us),
            (
                ReleaseKind::Sporadic,
                ReleaseSource::TopicRate,
                Some(50_000)
            )
        );
        let cap = get(&r, "/n/by_max_rate");
        assert_eq!(
            (cap.release, cap.source, cap.period_us),
            (ReleaseKind::Sporadic, ReleaseSource::MaxRate, Some(250_000))
        );

        let served = get(&r, "/n/by_budget");
        assert_eq!(
            (served.release, served.source),
            (ReleaseKind::Served, ReleaseSource::Budget)
        );
        assert_eq!(
            (served.budget_us, served.budget_period_us, served.period_us),
            (Some(2_000), Some(100_000), None)
        );
        assert_eq!(served.deadline_us, Some(100_000));

        let none = get(&r, "/n/unbounded");
        assert_eq!(
            (none.release, none.source, none.period_us, none.deadline_us),
            (ReleaseKind::Unanalysable, ReleaseSource::None, None, None)
        );

        let serve = get(&r, "/n/serve");
        assert_eq!(
            (serve.kind, serve.release, serve.period_us),
            (
                CallbackKind::ServiceServer,
                ReleaseKind::Sporadic,
                Some(200_000)
            )
        );
        // A server's `max_response` is its stated deadline.
        assert_eq!(
            (serve.deadline_us, serve.deadline_stated),
            (Some(5_000), true)
        );

        // The client's responses follow the 10 Hz timer's requests.
        let ask = get(&r, "/n/ask");
        assert_eq!(
            (ask.kind, ask.release, ask.source, ask.period_us),
            (
                CallbackKind::ServiceClient,
                ReleaseKind::Sporadic,
                ReleaseSource::Request,
                Some(100_000)
            )
        );
    }

    /// C is the RFC-0078 bound at the path boundary, absent without one.
    #[test]
    fn exec_time_comes_from_the_wcet_profile_and_is_never_invented() {
        use crate::wcet::{BoundaryWcet, WcetProfile};
        let profile = WcetProfile {
            cpu: s("cortex-m7"),
            clock_hz: Some(1_000_000),
            profile: s("release"),
            measured_at_commit: s("a1b2c3d4e5f6"),
            counter_valid: true,
            source: s("nros.wcet.measurements/1"),
            margin_percent: Some(0.0),
            coverage: None,
            boundaries: [(
                s("/n/react"),
                BoundaryWcet {
                    min_observed_cycles: 100,
                    max_observed_cycles: 1_500,
                    iterations: 10,
                    bound_cycles: None,
                },
            )]
            .into_iter()
            .collect(),
        };
        let r = task_records(&fixture(), Some(&profile));
        assert_eq!(get(&r, "/n/by_gap").exec_us, Some(1_500));
        assert_eq!(get(&r, "/n/tick").exec_us, None);
        assert!(
            task_records(&fixture(), None)
                .iter()
                .all(|t| t.exec_us.is_none())
        );
    }

    /// Acceptance: on an image without `supervise:`, an event input with no
    /// rate, minimum inter-arrival or budget is reported by name.
    #[test]
    fn an_unbounded_input_is_reported_by_name_without_supervise() {
        let model = fixture();
        let records = check_model(&model).expect("no supervise: reported, not refused");
        let d = unanalysable_degradations(&records);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].node, "/n/unbounded");
        assert_eq!(d[0].dim, "unanalysable");
        assert!(d[0].reason.contains("min_interarrival"), "{}", d[0].reason);
        assert!(d[0].reason.contains("budget_period"), "{}", d[0].reason);
    }

    /// Acceptance: the same input is refused by name, naming the two keys
    /// that would fix it, on an image whose contract supervises a deadline.
    #[test]
    fn an_unbounded_input_is_refused_by_name_on_a_safety_image() {
        let mut model = fixture();
        let react = model.contracts.node_paths.get_mut("/n/react").unwrap();
        react.supervise = Some(SuperviseContract {
            route_deadline_ms: 30.0,
            reaction: s("/n/tick"),
            mechanism: SuperviseMechanism::Timer,
        });
        let err = check_model(&model).expect_err("a safety image refuses it");
        assert_eq!(err.supervised, BTreeSet::from([s("/n")]));
        assert_eq!(
            err.unanalysable
                .iter()
                .map(|t| t.callback.as_str())
                .collect::<Vec<_>>(),
            ["/n/unbounded"]
        );
        let msg = err.to_string();
        for needle in [
            "`/n/unbounded` (subscription)",
            "`min_interarrival: <duration>`",
            "`budget: <duration>` + `budget_period: <duration>`",
            "nodes.</n>.sub.unbounded",
            "`supervise:` on `/n`",
        ] {
            assert!(msg.contains(needle), "missing {needle:?} in:\n{msg}");
        }

        // Declaring either key fixes it.
        model
            .contracts
            .sub_endpoints
            .get_mut("/n/unbounded")
            .unwrap()
            .min_interarrival_ms = Some(100.0);
        check_model(&model).expect("bounded now");
    }

    /// A client whose issuing path reads an unbounded input inherits the
    /// gap, and the refusal says where to bound it.
    #[test]
    fn a_client_inherits_the_release_of_the_paths_that_issue_its_requests() {
        let mut model = fixture();
        let tick = model.contracts.node_paths.get_mut("/n/tick").unwrap();
        tick.trigger = Some(EffectiveTrigger::Input(vec![s("unbounded")]));
        tick.input = vec![s("/n/unbounded")];
        let r = task_records(&model, None);
        assert_eq!(get(&r, "/n/ask").release, ReleaseKind::Unanalysable);
        let d = unanalysable_degradations(&r);
        let ask = d.iter().find(|d| d.node == "/n/ask").expect("reported");
        assert!(
            ask.reason.contains("bound the paths that issue them"),
            "{}",
            ask.reason
        );
        // Bound the input, and the client is sporadic at its T_min.
        model
            .contracts
            .sub_endpoints
            .get_mut("/n/unbounded")
            .unwrap()
            .budget_ms = Some(1.0);
        model
            .contracts
            .sub_endpoints
            .get_mut("/n/unbounded")
            .unwrap()
            .budget_period_ms = Some(40.0);
        let r = task_records(&model, None);
        let ask = get(&r, "/n/ask");
        assert_eq!(
            (ask.release, ask.period_us),
            (ReleaseKind::Sporadic, Some(40_000))
        );
    }
}
