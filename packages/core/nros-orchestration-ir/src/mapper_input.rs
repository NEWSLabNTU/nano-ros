//! phase-457 W2 - `SystemModel -> MapperInput` is ONE call into
//! `ros_launch_manifest_derive`, the derivation play_launch calls too
//! (rlm design issue #52, play_launch phase 78).
//!
//! phase-296 W5.1 wrote a second derivation here, over the same model. It
//! read a timer's rate off the first output's rate PROMISE, the label
//! rather than the effective criticality, and a concurrency rule that
//! disagreed with play_launch's on `exclusive: [[a, b], [c]]`; it passed
//! `chains: Vec::new()`, so the "chain-aware" tier order was the
//! criticality-bucket fallback on every image this repository has ever
//! built. The tier a node got on Zephyr and the `SCHED_FIFO` priority it
//! got on Linux were computed from different inputs and agreed by
//! coincidence. This module is now the adapter around the shared function
//! and holds no rule of its own but one (`DeriveFacts`, below).
//!
//! What stays this side's: the `[wcet]` profile. `DeriveFacts` is the one
//! input the model does not carry because it differs per platform -
//! play_launch fills `node_exec_ms` from a platform file's `budget_us`,
//! nano-ros fills `path_exec_ms` from an RFC-0078 profile, keyed at rlm's
//! own boundary identity (`"<node fqn>/<path>"`, the key
//! `boundaries_without_wcet` reports). Nothing else is added here, and no
//! WCET is ever invented.
//!
//! What the shared function refuses to guess: a path the model carries no
//! `trigger` for is `Unclassified`, never a timer, and
//! [`paths_without_trigger`] names it so `codegen-system` can say so. A
//! model resolved before rlm v0.1.37 therefore ranks nothing LOUDLY
//! instead of ranking something wrongly.

use ros_launch_manifest_derive::{DeriveFacts, DeriveReport};
use ros_launch_manifest_model::SystemModel;
use ros_launch_manifest_sched::{MapperInput, RankedPlan, chain_aware_rank};

use crate::wcet::WcetProfile;

/// The RFC-0078 profile as the shared derivation's per-path cost fact.
///
/// `WcetProfile::boundaries` is already keyed by rlm's boundary identity,
/// so the map is a filter, not a translation: a boundary that declares no
/// BOUND (an observation alone) or a profile with no usable clock rate
/// yields no entry, and absent is not zero - rlm counts the boundary as
/// undeclared and `ChainFeasibleWithoutWcet` says so. `node_exec_ms` stays
/// empty: nothing on this side speaks a whole-node budget.
fn derive_facts(wcet: Option<&WcetProfile>) -> DeriveFacts {
    let Some(profile) = wcet else {
        return DeriveFacts::default();
    };
    DeriveFacts {
        path_exec_ms: profile
            .boundaries
            .keys()
            .filter_map(|b| profile.exec_ms(b).map(|ms| (b.clone(), ms)))
            .collect(),
        ..DeriveFacts::default()
    }
}

/// Derive a [`MapperInput`] from the model through the shared derivation.
pub fn mapper_input_from_model(model: &SystemModel) -> MapperInput {
    mapper_input_from_model_with_wcet(model, None)
}

/// As [`mapper_input_from_model`], with a selected RFC-0078 measurement profile
/// supplying `exec_ms` for the boundaries it declares.
///
/// Separate entry point rather than a changed signature so that ABSENT remains
/// the default at the API level too: a caller that knows nothing about WCETs
/// gets exactly what it got before, which is `None` everywhere. Which profile
/// is selected is deliberately not decided here - RFC-0078 leaves that to
/// `system.toml` / SystemModel, and this function takes the answer.
pub fn mapper_input_from_model_with_wcet(
    model: &SystemModel,
    wcet: Option<&WcetProfile>,
) -> MapperInput {
    mapper_input_and_report(model, wcet).0
}

/// The derivation plus the shared [`DeriveReport`]: what the model could not
/// tell it. The caller that has somewhere to print belongs here; the two
/// wrappers above are for the callers that do not.
pub fn mapper_input_and_report(
    model: &SystemModel,
    wcet: Option<&WcetProfile>,
) -> (MapperInput, DeriveReport) {
    // `MapperNode::scope` is the NAMESPACE a `manual` mapper's
    // `[[assign]] scope = "/perception"` selector matches against. It used to
    // be the model's FILE-scope key and this function patched it back on the
    // way out; rlm v0.1.40 (design issue #52 R5) derives it in the crate, so
    // the derivation is read through unchanged and both consumers agree by
    // construction rather than by two copies of the same rule.
    ros_launch_manifest_derive::mapper_input_from_model(model, &derive_facts(wcet))
}

/// `"<node fqn>/<path>"` of every node path the model carries no trigger fact
/// for. Each is `Unclassified` and ranks nothing; `codegen-system` prints the
/// list so an unclassified path is said once, by name, rather than dropped out
/// of the schedule in silence.
pub fn paths_without_trigger(model: &SystemModel) -> Vec<String> {
    mapper_input_and_report(model, None).1.paths_without_trigger
}

/// Convenience: derive the input and run the shared platform-agnostic core,
/// returning the priorityless [`RankedPlan`] the RTOS realizer (W5.2) consumes.
pub fn rank_from_model(model: &SystemModel) -> RankedPlan {
    chain_aware_rank(&mapper_input_from_model(model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ros_launch_manifest_model::{
        Contracts, Execution, NodeInstance, PathContract, Structure, SystemModel, TopicContract,
    };
    use ros_launch_manifest_sched::{EffectiveTrigger, mapper::Criticality};
    use std::collections::BTreeMap;

    /// RFC-0078 - a declared profile reaches `MapperPath::exec_ms`, and nothing
    /// else does.
    ///
    /// The fixture's boundaries are `/sensor/acquire` and `/planner/plan`, which
    /// is the real key shape: a ROS node FQN begins with `/` and may carry
    /// namespaces. Writing this test is what caught a validator rule that
    /// required exactly two segments and would have rejected every boundary in
    /// the tree.
    #[test]
    fn a_declared_wcet_reaches_exec_ms_and_an_undeclared_one_stays_absent() {
        use crate::wcet::{BoundaryWcet, WcetProfile};

        let model = model_with_two_nodes();
        let profile = WcetProfile {
            cpu: "cortex-m4f".into(),
            clock_hz: Some(1_000_000),
            profile: "release".into(),
            measured_at_commit: "a1b2c3d4e5f6".into(),
            counter_valid: true,
            source: "nros.wcet.measurements/1".into(),
            // A margin is what turns the high-water mark into a bound. Without
            // it this profile would declare an observation and yield NO
            // exec_ms - which is the behaviour `an_observation_alone_reaches_
            // nothing` below pins.
            margin_percent: Some(0.0),
            coverage: Some("fixture: fixed inputs".into()),
            boundaries: BTreeMap::from([(
                "/sensor/acquire".to_string(),
                BoundaryWcet {
                    min_observed_cycles: 1_000,
                    max_observed_cycles: 2_500,
                    iterations: 100,
                    bound_cycles: None,
                },
            )]),
        };
        assert!(profile.validate().is_empty(), "{:?}", profile.validate());

        let input = mapper_input_from_model_with_wcet(&model, Some(&profile));
        let exec = |node: &str, path: &str| {
            input
                .nodes
                .iter()
                .find(|n| n.name == node)
                .and_then(|n| n.paths.iter().find(|p| p.name == path))
                .and_then(|p| p.exec_ms)
        };

        // 2_500 cycles at 1 MHz = 2.5 ms.
        let declared = exec("/sensor", "acquire").expect("declared boundary must carry exec_ms");
        assert!((declared - 2.5).abs() < 1e-9, "got {declared}");

        // The other boundary was not declared. Absent, not zero - rlm will count
        // it as undeclared and say so via ChainFeasibleWithoutWcet.
        assert_eq!(
            exec("/planner", "plan"),
            None,
            "an undeclared boundary must stay absent; a zero here would claim \
             measured headroom nobody has"
        );
    }

    /// An observation with no bound reaches `exec_ms` as `None`, even though
    /// the boundary IS declared and the rate IS known.
    ///
    /// This is the research finding wired end to end: a maximum observed over N
    /// runs is a high-water mark, and letting it through would hand rlm an
    /// under-estimate that looks measured.
    #[test]
    fn an_observation_alone_reaches_nothing() {
        use crate::wcet::{BoundaryWcet, WcetProfile};

        let model = model_with_two_nodes();
        let profile = WcetProfile {
            cpu: "cortex-m4f".into(),
            clock_hz: Some(1_000_000),
            profile: "release".into(),
            measured_at_commit: "a1b2c3d4e5f6".into(),
            counter_valid: true,
            source: "nros.wcet.measurements/1".into(),
            margin_percent: None, // no bound derived from the observation
            coverage: None,
            boundaries: BTreeMap::from([(
                "/sensor/acquire".to_string(),
                BoundaryWcet {
                    min_observed_cycles: 1_000,
                    max_observed_cycles: 2_500,
                    iterations: 100,
                    bound_cycles: None,
                },
            )]),
        };
        assert!(
            profile.validate().is_empty(),
            "the declaration is well-formed"
        );

        let input = mapper_input_from_model_with_wcet(&model, Some(&profile));
        assert!(
            input
                .nodes
                .iter()
                .flat_map(|n| &n.paths)
                .all(|p| p.exec_ms.is_none()),
            "an observation without a declared bound must not reach exec_ms"
        );
    }

    /// Without a profile the derivation invents nothing - absent is the default
    /// at the API level, not just in the data.
    #[test]
    fn no_profile_means_no_exec_ms_anywhere() {
        let model = model_with_two_nodes();
        let input = mapper_input_from_model(&model);
        assert!(
            input
                .nodes
                .iter()
                .flat_map(|n| &n.paths)
                .all(|p| p.exec_ms.is_none()),
            "the no-profile path must invent nothing"
        );
    }

    fn model_with_two_nodes() -> SystemModel {
        let mut nodes = indexmap::IndexMap::new();
        nodes.insert(
            "/sensor".to_string(),
            NodeInstance {
                scope: "bringup.launch.xml".to_string(),
                criticality: Some("high".to_string()),
                ..Default::default()
            },
        );
        nodes.insert(
            "/planner".to_string(),
            NodeInstance {
                scope: "bringup.launch.xml".to_string(),
                criticality: Some("medium".to_string()),
                ..Default::default()
            },
        );

        let mut node_paths = BTreeMap::new();
        // /sensor: a 20 Hz timer publishing /sensor/scan. The rate lives on the
        // TRIGGER; no publisher rate promise is involved, and none is declared.
        node_paths.insert(
            "/sensor/acquire".to_string(),
            PathContract {
                input: vec![],
                output: vec!["/sensor/scan".to_string()],
                trigger: Some(EffectiveTrigger::Timer { rate_hz: 20.0 }),
                max_latency_ms: Some(5.0),
                ..Default::default()
            },
        );
        // /planner: event-driven on /planner/objects_in -> /planner/cmd.
        node_paths.insert(
            "/planner/plan".to_string(),
            PathContract {
                input: vec!["/planner/objects_in".to_string()],
                output: vec!["/planner/cmd".to_string()],
                trigger: Some(EffectiveTrigger::Input(vec![
                    "/planner/objects_in".to_string(),
                ])),
                max_latency_ms: Some(30.0),
                ..Default::default()
            },
        );

        SystemModel {
            structure: Structure {
                nodes,
                ..Default::default()
            },
            contracts: Contracts {
                node_paths,
                topics: BTreeMap::<String, TopicContract>::new(),
                ..Default::default()
            },
            execution: Execution::default(),
            ..Default::default()
        }
    }

    /// Phase 434 - the three fields play_launch carried across the model
    /// boundary for this crate reach the mapper. Before this they stopped at
    /// the model: `max_jitter_ms`/`miss` were filled `None` here, and the
    /// concurrency relation was never read at all, so an author's claim that
    /// two callbacks may run at once - which is what makes a summed chain
    /// latency optimistic and a per-thread reservation unsound - was invisible
    /// on this side.
    #[test]
    fn contract_seams_reach_the_mapper() {
        use ros_launch_manifest_sched::{ConcurrencyContract, MapperMiss};
        let mut model = model_with_two_nodes();
        model
            .contracts
            .node_paths
            .get_mut("/planner/plan")
            .unwrap()
            .max_jitter_ms = Some(4.0);
        model
            .contracts
            .node_paths
            .get_mut("/planner/plan")
            .unwrap()
            .miss = Some(MapperMiss {
            tolerate_n: Some(1),
            ..Default::default()
        });
        // /planner has two paths; declaring `exclusive: [[plan]]` leaves the
        // other free to run concurrently - a claim of MORE concurrency.
        model.contracts.node_paths.insert(
            "/planner/telemetry".to_string(),
            PathContract {
                input: vec![],
                output: vec!["/planner/stats".to_string()],
                trigger: Some(EffectiveTrigger::Timer { rate_hz: 1.0 }),
                ..Default::default()
            },
        );
        model.contracts.node_concurrency.insert(
            "/planner".to_string(),
            ConcurrencyContract {
                exclusive: vec![vec!["plan".to_string()]],
            },
        );

        let input = mapper_input_from_model(&model);
        let planner = input.nodes.iter().find(|n| n.name == "/planner").unwrap();
        let plan = planner.paths.iter().find(|p| p.name == "plan").unwrap();
        assert_eq!(plan.max_jitter_ms, Some(4.0));
        assert_eq!(plan.miss.as_ref().and_then(|m| m.tolerate_n), Some(1));
        assert!(
            planner.claims_concurrency,
            "a declared exclusion that leaves a path out claims concurrency"
        );
        let sensor = input.nodes.iter().find(|n| n.name == "/sensor").unwrap();
        assert!(
            !sensor.claims_concurrency,
            "no declaration means everything serialises"
        );
    }

    #[test]
    fn derives_nodes_paths_and_triggers() {
        let model = model_with_two_nodes();
        let input = mapper_input_from_model(&model);

        assert_eq!(input.nodes.len(), 2);
        // No `contracts.scope_paths` in this fixture, so there is no budget to
        // derive a chain for; the shared function resolves one per scope path.
        assert!(input.chains.is_empty(), "no scope path, no chain");

        let sensor = input.nodes.iter().find(|n| n.name == "/sensor").unwrap();
        assert_eq!(sensor.criticality, Some(Criticality::High));
        assert_eq!(sensor.paths.len(), 1);
        assert_eq!(
            sensor.paths[0].effective_trigger,
            EffectiveTrigger::Timer { rate_hz: 20.0 }
        );
        // The fastest timer among the node's paths reaches the node.
        assert_eq!(sensor.rate_hz, Some(20.0));

        let planner = input.nodes.iter().find(|n| n.name == "/planner").unwrap();
        assert_eq!(planner.criticality, Some(Criticality::Medium));
        assert_eq!(
            planner.paths[0].effective_trigger,
            EffectiveTrigger::Input(vec!["/planner/objects_in".to_string()])
        );
        assert_eq!(planner.paths[0].max_latency_ms, Some(30.0));
        assert_eq!(planner.rate_hz, None, "no timer path, no rate");
    }

    /// phase-457 W2, W5 - `MapperNode::scope` is the NAMESPACE, not the
    /// model's file-scope key.
    ///
    /// The mapper matches a `[[assign]] scope =` selector against a ROS
    /// namespace, while `NodeInstance::scope` names the launch FILE a node
    /// was declared in. W2 patched the difference on this side and
    /// play_launch patched it on its own (phase 78 W2); rlm v0.1.40 closes
    /// it in the crate (design issue #52 R5) and both patches are gone.
    ///
    /// The test stays, and is worth more now than when it guarded our own
    /// loop: it asserts the OUTCOME rather than the mechanism, so it is what
    /// catches an upstream regression in a pin bump instead of at the point
    /// where a tier selector silently matches nothing.
    #[test]
    fn scope_is_the_namespace_not_the_file_scope_key() {
        let mut model = model_with_two_nodes();
        model.structure.nodes.insert(
            "/perception/lidar".to_string(),
            NodeInstance {
                scope: "bringup.launch.xml".to_string(),
                ..Default::default()
            },
        );
        let input = mapper_input_from_model(&model);
        let scope_of = |name: &str| {
            input
                .nodes
                .iter()
                .find(|n| n.name == name)
                .map(|n| n.scope.clone())
        };
        assert_eq!(scope_of("/perception/lidar"), Some("/perception".into()));
        assert_eq!(scope_of("/sensor"), Some("/".into()));
    }

    /// phase-457 W2 - the model carries NO trigger for a path, so the path is
    /// `Unclassified`, ranks nothing, and is NAMED. The old derivation read an
    /// empty `input` as a timer at the first output's promised rate, which is
    /// how a `once` map loader became a periodic task.
    #[test]
    fn a_path_without_a_trigger_is_named_and_never_a_timer() {
        let mut model = model_with_two_nodes();
        model
            .contracts
            .node_paths
            .get_mut("/sensor/acquire")
            .unwrap()
            .trigger = None;

        let (input, report) = mapper_input_and_report(&model, None);
        let sensor = input.nodes.iter().find(|n| n.name == "/sensor").unwrap();
        assert_eq!(
            sensor.paths[0].effective_trigger,
            EffectiveTrigger::Unclassified,
            "an empty `input` is not a timer"
        );
        assert_eq!(sensor.rate_hz, None);
        assert_eq!(report.paths_without_trigger, vec!["/sensor/acquire"]);
        assert_eq!(paths_without_trigger(&model), vec!["/sensor/acquire"]);
        assert!(
            paths_without_trigger(&model_with_two_nodes()).is_empty(),
            "a fully classified model reports nothing"
        );
    }

    /// phase-457 W2, the point of the wave - an ISLAND-shaped model (two timer
    /// loops, a fast one and a slow one) ranks by the timers' periods with NO
    /// publisher rate promise anywhere in it.
    ///
    /// Until this wave the period was reconstructed from the first output's
    /// rate promise, and the island's promises happened to equal its
    /// timers - the derived order was right by a redundant declaration the
    /// resolver itself flags as `derivable-min-rate`. Delete the promises and
    /// the old derivation ranked nothing at all. The new one ranks from the
    /// trigger, which is where the rate lives.
    #[test]
    fn an_island_shaped_model_ranks_from_the_timers_with_no_rate_promises() {
        let node = |criticality: &str| NodeInstance {
            scope: "island.launch.xml".to_string(),
            criticality: Some(criticality.to_string()),
            ..Default::default()
        };
        let mut nodes = indexmap::IndexMap::new();
        // Declaration order is deliberately the reverse of the rank order:
        // a pass-through would return them as declared.
        nodes.insert("/telem_node".to_string(), node("high"));
        nodes.insert("/control_node".to_string(), node("high"));

        let timer = |rate_hz: f64, out: &str, budget: f64| PathContract {
            input: vec![],
            output: vec![out.to_string()],
            trigger: Some(EffectiveTrigger::Timer { rate_hz }),
            max_latency_ms: Some(budget),
            ..Default::default()
        };
        let mut node_paths = BTreeMap::new();
        node_paths.insert(
            "/control_node/loop".to_string(),
            timer(30.0, "/control_node/cmd", 5.0),
        );
        node_paths.insert(
            "/telem_node/loop".to_string(),
            timer(10.0, "/telem_node/status", 100.0),
        );

        let model = SystemModel {
            structure: Structure {
                nodes,
                ..Default::default()
            },
            contracts: Contracts {
                node_paths,
                // No `pub_endpoints` at all: not one rate promise in the model.
                ..Default::default()
            },
            ..Default::default()
        };

        let input = mapper_input_from_model(&model);
        let rate = |name: &str| {
            input
                .nodes
                .iter()
                .find(|n| n.name == name)
                .and_then(|n| n.rate_hz)
        };
        assert_eq!(rate("/control_node"), Some(30.0));
        assert_eq!(rate("/telem_node"), Some(10.0));

        let ranked = rank_from_model(&model);
        let pos = |name: &str| ranked.items.iter().position(|i| i.node == name);
        assert!(
            pos("/control_node") < pos("/telem_node"),
            "the 30 Hz loop must outrank the 10 Hz one on period alone: {:?}",
            ranked.items
        );
    }

    #[test]
    fn feeds_the_agnostic_core() {
        // The derived input runs through the shared core. Proves the
        // SystemModel -> MapperInput -> chain_aware_rank pipeline end to end.
        let model = model_with_two_nodes();
        let ranked = rank_from_model(&model);
        // Both nodes' paths are ranked; the high-criticality sensor outranks
        // the medium planner.
        assert!(!ranked.items.is_empty());
        let sensor_pos = ranked.items.iter().position(|i| i.node == "/sensor");
        let planner_pos = ranked.items.iter().position(|i| i.node == "/planner");
        assert!(sensor_pos.is_some() && planner_pos.is_some());
        assert!(
            sensor_pos < planner_pos,
            "High-criticality /sensor must outrank medium /planner: {:?}",
            ranked.items
        );
    }
}
