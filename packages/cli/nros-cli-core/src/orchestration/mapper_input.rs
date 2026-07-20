//! phase-296 W5.1 — `SystemModel → MapperInput` derivation (RFC-0052
//! §"nano-ros execution modeling").
//!
//! nano-ros does its OWN causality/execution modeling: it derives the mapper
//! INPUT from the model's **input** layers (structure + contracts) and feeds
//! the shared platform-agnostic core
//! ([`ros_launch_manifest_sched::chain_aware_rank`]) — it does NOT consume any
//! resolved sched plan (there is none; `execution.sched` was reverted, rlm
//! `f090400`). The core returns a priorityless
//! [`ros_launch_manifest_sched::RankedPlan`]; the RTOS realizer (W5.2) turns
//! that into kernel-feature scheduling.
//!
//! v1 scope: the **node side** — `MapperNode` (scope, criticality, per-path
//! trigger/latency facts) from `structure.nodes` + `contracts.node_paths` +
//! `contracts.pub_endpoints`. Chains are **empty** (the model carries no chain
//! declarations today), so the core degrades to criticality-bucketed RM/DM —
//! the design's graceful-degradation path. Chain-declaration input is a later
//! wave.

use ros_launch_manifest_model::SystemModel;
use ros_launch_manifest_sched::{
    MapperInput, MapperNode, RankedPlan, chain::EffectiveTrigger, chain::MapperPath,
    chain_aware_rank, mapper::Criticality,
};

/// Parse the model's advisory criticality string (`high`|`medium`|`low`) into
/// the mapper's [`Criticality`]. Unknown/absent → `None` (the mapper treats a
/// node with no criticality as the lowest bucket).
fn parse_criticality(s: &str) -> Option<Criticality> {
    match s.trim().to_ascii_lowercase().as_str() {
        "high" => Some(Criticality::High),
        "medium" | "med" => Some(Criticality::Medium),
        "low" => Some(Criticality::Low),
        _ => None,
    }
}

/// The declared publisher rate (Hz) for an endpoint ref, if the model carries a
/// `min_rate_hz` contract for it. Used as the fire rate of a periodic (timer)
/// path — a path with no `input` fires on a clock, and its output endpoint's
/// contracted rate is the honest "how often" fact.
fn pub_rate_hz(model: &SystemModel, endpoint_ref: &str) -> Option<f64> {
    model
        .contracts
        .pub_endpoints
        .get(endpoint_ref)
        .and_then(|c| c.min_rate_hz)
}

/// Derive one node's causal paths from `contracts.node_paths`. A node path is
/// keyed `"<node_fqn>/<path_name>"`; a path belongs to `fqn` when its key has
/// that prefix and the remainder is a single segment (the path name).
fn node_paths_for(model: &SystemModel, fqn: &str) -> Vec<MapperPath> {
    let prefix = format!("{fqn}/");
    let mut out = Vec::new();
    for (path_ref, pc) in &model.contracts.node_paths {
        let Some(name) = path_ref.strip_prefix(&prefix) else {
            continue;
        };
        // Guard against a longer node FQN prefix-matching a shorter one: the
        // path name is a single segment (no further `/`).
        if name.contains('/') {
            continue;
        }
        // Trigger: an empty `input` is a timer/periodic path (fires on a
        // clock at the output's contracted rate); otherwise it is event-driven
        // on its input endpoints.
        let effective_trigger = if pc.input.is_empty() {
            let rate_hz = pc
                .output
                .iter()
                .find_map(|o| pub_rate_hz(model, o))
                .unwrap_or(0.0);
            EffectiveTrigger::Timer { rate_hz }
        } else {
            EffectiveTrigger::Input(pc.input.clone())
        };
        out.push(MapperPath {
            name: name.to_string(),
            effective_trigger,
            max_latency_ms: pc.max_latency_ms,
            // No WCET in the contract vocabulary — never invent one.
            exec_ms: None,
            inputs: pc.input.clone(),
            outputs: pc.output.clone(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Derive a [`MapperInput`] from the SystemModel's input layers. Nodes come
/// from `structure.nodes` + their `contracts.node_paths`; chains are empty in
/// v1 (the model carries no chain declarations).
pub fn mapper_input_from_model(model: &SystemModel) -> MapperInput {
    let mut nodes = Vec::with_capacity(model.structure.nodes.len());
    for (fqn, node) in &model.structure.nodes {
        let criticality = node.criticality.as_deref().and_then(parse_criticality);
        nodes.push(MapperNode {
            name: fqn.clone(),
            scope: node.scope.clone(),
            criticality,
            paths: node_paths_for(model, fqn),
            ..Default::default()
        });
    }
    MapperInput {
        nodes,
        legacy: None,
        chains: Vec::new(),
    }
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
        Contracts, Execution, NodeInstance, PathContract, PubContract, Structure, SystemModel,
        TopicContract,
    };
    use std::collections::BTreeMap;

    fn model_with_two_nodes() -> SystemModel {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "/sensor".to_string(),
            NodeInstance {
                scope: "/".to_string(),
                criticality: Some("high".to_string()),
                ..Default::default()
            },
        );
        nodes.insert(
            "/planner".to_string(),
            NodeInstance {
                scope: "/".to_string(),
                criticality: Some("medium".to_string()),
                ..Default::default()
            },
        );

        let mut pub_endpoints = BTreeMap::new();
        pub_endpoints.insert(
            "/sensor/scan".to_string(),
            PubContract {
                min_rate_hz: Some(20.0),
                ..Default::default()
            },
        );

        let mut node_paths = BTreeMap::new();
        // /sensor: periodic (empty input) publishing /sensor/scan at 20 Hz.
        node_paths.insert(
            "/sensor/acquire".to_string(),
            PathContract {
                input: vec![],
                output: vec!["/sensor/scan".to_string()],
                max_latency_ms: Some(5.0),
                ..Default::default()
            },
        );
        // /planner: event-driven on /planner/objects_in → /planner/cmd.
        node_paths.insert(
            "/planner/plan".to_string(),
            PathContract {
                input: vec!["/planner/objects_in".to_string()],
                output: vec!["/planner/cmd".to_string()],
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
                pub_endpoints,
                node_paths,
                topics: BTreeMap::<String, TopicContract>::new(),
                ..Default::default()
            },
            execution: Execution::default(),
            ..Default::default()
        }
    }

    #[test]
    fn derives_nodes_paths_and_triggers() {
        let model = model_with_two_nodes();
        let input = mapper_input_from_model(&model);

        assert_eq!(input.nodes.len(), 2);
        assert!(input.chains.is_empty(), "v1 carries no chains");

        let sensor = input.nodes.iter().find(|n| n.name == "/sensor").unwrap();
        assert_eq!(sensor.criticality, Some(Criticality::High));
        assert_eq!(sensor.paths.len(), 1);
        // Empty-input path → Timer at the output's contracted 20 Hz.
        assert_eq!(
            sensor.paths[0].effective_trigger,
            EffectiveTrigger::Timer { rate_hz: 20.0 }
        );

        let planner = input.nodes.iter().find(|n| n.name == "/planner").unwrap();
        assert_eq!(planner.criticality, Some(Criticality::Medium));
        // Non-empty input → event-driven.
        assert_eq!(
            planner.paths[0].effective_trigger,
            EffectiveTrigger::Input(vec!["/planner/objects_in".to_string()])
        );
        assert_eq!(planner.paths[0].max_latency_ms, Some(30.0));
    }

    #[test]
    fn feeds_the_agnostic_core() {
        // The derived input runs through the shared core (no chains → the
        // criticality-bucketed RM/DM degradation path). Proves the
        // SystemModel → MapperInput → chain_aware_rank pipeline end to end.
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
