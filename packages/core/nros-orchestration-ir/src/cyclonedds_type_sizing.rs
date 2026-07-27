//! Issue 0284 — model-derived CycloneDDS type-registry sizing.
//!
//! The CycloneDDS backend memoises one DDS type descriptor per DISTINCT ROS type
//! name in a bounded [`heapless::FnvIndexMap`] of `NROS_CYCLONEDDS_MAX_TYPES`
//! slots (default 32, MUST be a power of two). Before this module the knob was
//! discovered at RUNTIME: a bringup that registers more distinct types than the
//! table holds boots and dies on the first over-capacity `get_or_build` with
//! `BuildError::RegistryFull`.
//!
//! The SystemModel names every topic / service / action an entry wires, with its
//! type, so a bake can count the DISTINCT types the image will register and
//! (a) size the knob and (b) refuse an image whose capacity is known-too-small —
//! exactly the shape of the [`crate::executor_sizing`] callback-table work
//! (issue 0257).
//!
//! # Why the model is COMPLETE here (unlike callback sizing)
//!
//! The callback count is a LOWER bound because the model has no timer / guard-
//! condition entity. Types are different: **only** pub/sub/service/action
//! endpoints register DDS types — timers and guard conditions register none — so
//! the model wiring, which names every one of those, sees the whole type set.
//! No source-metadata union is needed for correctness.
//!
//! # The expansion — one interface is more than one DDS type
//!
//! `nros-node` registers, per entity kind (see
//! `packages/core/nros-node/src/executor/node.rs`
//! `register_type::<…>()` sites):
//!
//! - **message** (publisher OR subscriber): the message type — **1** name
//!   (`node.rs:227` / `:354`).
//! - **service** (server OR client): `<Srv>_Request` + `<Srv>_Response` — **2**
//!   names (`node.rs:483-484` / `:547-548`).
//! - **action** (server OR client): the eight envelopes `_Goal`, `_Result`,
//!   `_Feedback`, `_SendGoal_{Request,Response}`, `_GetResult_{Request,Response}`,
//!   `_FeedbackMessage` — **8** names (`node.rs:932-939`).
//! - the fixed `action_msgs` protocol types (`CancelGoal_{Request,Response}`,
//!   `GoalStatusArray`) are registered ONCE when the entry has ANY action —
//!   **+3** shared (`A::register_protocol_types`, `node.rs:172`).
//!
//! The factors below MIRROR those register sites. A change to how many types a
//! kind registers must update them in lockstep — the unit test
//! `expansion_matches_documented_factors` pins the arithmetic so a silent drift
//! breaks the build.

use std::collections::BTreeSet;

use ros_launch_manifest_model::SystemModel;

/// The `nros-rmw-cyclonedds` build-time default for `NROS_CYCLONEDDS_MAX_TYPES`
/// (`type_registry::MAX_TYPES`). Mirrored here because the bake is HOST code that
/// runs before the crate's compile. Keep in sync with
/// `packages/dds/nros-rmw-cyclonedds/src/type_registry.rs`.
pub const DEFAULT_MAX_TYPES: usize = 32;

/// Distinct DDS type names one MESSAGE interface registers.
const TYPES_PER_MSG: usize = 1;
/// Distinct DDS type names one SERVICE interface registers (`_Request`,
/// `_Response`).
const TYPES_PER_SRV: usize = 2;
/// Distinct DDS type names one ACTION interface registers (the eight envelopes).
const TYPES_PER_ACTION: usize = 8;
/// The fixed `action_msgs` protocol types registered ONCE per entry that has any
/// action (`CancelGoal_{Request,Response}`, `GoalStatusArray`).
const ACTION_MSGS_SHARED: usize = 3;

/// Node FQN owning an endpoint ref (`"/ns/node/endpoint"` → `"/ns/node"`).
fn endpoint_node(ep: &str) -> &str {
    ep.rsplit_once('/').map(|(node, _)| node).unwrap_or(ep)
}

/// Expand distinct-interface counts into distinct DDS-type-name count (the crux;
/// see the module docs).
fn expand(distinct_msg: usize, distinct_srv: usize, distinct_action: usize) -> usize {
    let mut n = distinct_msg * TYPES_PER_MSG
        + distinct_srv * TYPES_PER_SRV
        + distinct_action * TYPES_PER_ACTION;
    if distinct_action > 0 {
        n += ACTION_MSGS_SHARED;
    }
    n
}

/// The number of DISTINCT DDS type names a CycloneDDS bringup registers for the
/// nodes `keep` selects (by node FQN). Complete (not a lower bound) — see the
/// module docs. `keep` returning `true` for every node counts the whole entry.
pub fn count_dds_types<F: FnMut(&str) -> bool>(model: &SystemModel, mut keep: F) -> usize {
    // Distinct type NAMES per kind. A msg type shared by a pub and a sub, or an
    // action_msgs type shared across actions, is registered once — the set
    // dedups it. Kind name-spaces don't collide, so per-kind sets suffice.
    let mut msg: BTreeSet<&str> = BTreeSet::new();
    let mut srv: BTreeSet<&str> = BTreeSet::new();
    let mut action: BTreeSet<&str> = BTreeSet::new();

    for w in model.structure.topics.values() {
        let participates = w
            .publishers
            .iter()
            .chain(w.subscribers.iter())
            .any(|ep| keep(endpoint_node(ep)));
        if participates {
            msg.insert(w.msg_type.as_str());
        }
    }
    for w in model.structure.services.values() {
        let participates = w
            .server
            .iter()
            .chain(w.client.iter())
            .any(|ep| keep(endpoint_node(ep)));
        if participates {
            srv.insert(w.srv_type.as_str());
        }
    }
    for w in model.structure.actions.values() {
        let participates = w
            .server
            .iter()
            .chain(w.client.iter())
            .any(|ep| keep(endpoint_node(ep)));
        if participates {
            action.insert(w.srv_type.as_str());
        }
    }

    expand(msg.len(), srv.len(), action.len())
}

/// The CycloneDDS `MAX_TYPES` a registered-type count needs: the smallest power
/// of two `>= counted` (`heapless::FnvIndexMap`'s constraint), never below the
/// build-time default so a small entry stays byte-identical. `0` when nothing is
/// registered (no emit / no check).
pub fn derive_max_types(counted: usize) -> usize {
    if counted == 0 {
        return 0;
    }
    counted.next_power_of_two().max(DEFAULT_MAX_TYPES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ros_launch_manifest_model::{ServiceWiring, TopicWiring};

    fn model_with(
        topics: Vec<(&str, &str, Vec<&str>, Vec<&str>)>,
        services: Vec<(&str, &str, Vec<&str>, Vec<&str>)>,
        actions: Vec<(&str, &str, Vec<&str>, Vec<&str>)>,
    ) -> SystemModel {
        let mut m = SystemModel::default();
        for (name, ty, pubs, subs) in topics {
            m.structure.topics.insert(
                name.into(),
                TopicWiring {
                    msg_type: ty.into(),
                    publishers: pubs.into_iter().map(Into::into).collect(),
                    subscribers: subs.into_iter().map(Into::into).collect(),
                },
            );
        }
        for (name, ty, srv, cli) in services {
            m.structure.services.insert(
                name.into(),
                ServiceWiring {
                    srv_type: ty.into(),
                    server: srv.into_iter().map(Into::into).collect(),
                    client: cli.into_iter().map(Into::into).collect(),
                },
            );
        }
        for (name, ty, srv, cli) in actions {
            m.structure.actions.insert(
                name.into(),
                ServiceWiring {
                    srv_type: ty.into(),
                    server: srv.into_iter().map(Into::into).collect(),
                    client: cli.into_iter().map(Into::into).collect(),
                },
            );
        }
        m
    }

    #[test]
    fn empty_model_counts_zero() {
        let m = SystemModel::default();
        assert_eq!(count_dds_types(&m, |_| true), 0);
        assert_eq!(derive_max_types(0), 0);
    }

    #[test]
    fn distinct_messages_dedup_across_pub_and_sub() {
        // Two topics, SAME type, one pub'd + one sub'd by different nodes.
        // Registered distinct msg types = 1.
        let m = model_with(
            vec![
                ("/a", "std_msgs/msg/Int32", vec!["/talker/a"], vec![]),
                ("/b", "std_msgs/msg/Int32", vec![], vec!["/listener/b"]),
            ],
            vec![],
            vec![],
        );
        assert_eq!(count_dds_types(&m, |_| true), 1);
    }

    #[test]
    fn expansion_matches_documented_factors() {
        // 2 distinct msgs + 1 service + 1 action.
        // = 2*1 + 1*2 + 1*8 + 3 (action_msgs) = 15.
        let m = model_with(
            vec![
                ("/chatter", "std_msgs/msg/Int32", vec!["/n/chatter"], vec![]),
                ("/pose", "geometry_msgs/msg/Pose", vec![], vec!["/n/pose"]),
            ],
            vec![(
                "/add",
                "example_interfaces/srv/AddTwoInts",
                vec!["/n/add"],
                vec![],
            )],
            vec![(
                "/fib",
                "example_interfaces/action/Fibonacci",
                vec!["/n/fib"],
                vec![],
            )],
        );
        assert_eq!(count_dds_types(&m, |_| true), 2 + 2 + 8 + 3);
    }

    #[test]
    fn action_msgs_shared_counted_once_across_actions() {
        // Two DISTINCT actions → 2*8 envelopes + 3 shared (once) = 19.
        let m = model_with(
            vec![],
            vec![],
            vec![
                (
                    "/fib",
                    "example_interfaces/action/Fibonacci",
                    vec!["/n/fib"],
                    vec![],
                ),
                (
                    "/look",
                    "tf2_msgs/action/LookupTransform",
                    vec!["/n/look"],
                    vec![],
                ),
            ],
        );
        assert_eq!(count_dds_types(&m, |_| true), 2 * 8 + 3);
    }

    #[test]
    fn keep_predicate_scopes_the_count() {
        let m = model_with(
            vec![
                ("/a", "pkg/msg/A", vec!["/n1/a"], vec![]),
                ("/b", "pkg/msg/B", vec!["/n2/b"], vec![]),
            ],
            vec![],
            vec![],
        );
        assert_eq!(count_dds_types(&m, |n| n == "/n1"), 1);
    }

    #[test]
    fn derive_rounds_up_to_power_of_two_never_below_default() {
        assert_eq!(derive_max_types(1), 32); // floored at the default
        assert_eq!(derive_max_types(32), 32);
        assert_eq!(derive_max_types(33), 64);
        assert_eq!(derive_max_types(65), 128);
        assert!(derive_max_types(200).is_power_of_two());
    }
}
