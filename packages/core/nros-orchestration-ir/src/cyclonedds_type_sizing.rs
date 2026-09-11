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
/// `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/type_registry.rs`.
pub const DEFAULT_MAX_TYPES: usize = 32;

/// The `descriptors.cpp` build-time default for
/// `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` (`kMaxRegisteredTypes`). Mirrored here
/// for the same reason [`DEFAULT_MAX_TYPES`] is: the bake is HOST code that runs
/// before that TU compiles. Keep in sync with
/// `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/descriptors.cpp`.
pub const DEFAULT_MAX_DESCRIPTOR_TYPES: usize = 256;

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

/// Issue 1268 — distinct DDS type names the SIX parameter services register.
///
/// They are the executor's own, not the model's: `param_services` puts a set of
/// six on every node, and each service registers `_Request` + `_Response`, so
/// twelve names — `rcl_interfaces::srv::dds_::{Get,Set,SetAtomically,List,
/// Describe,GetTypes}Parameters_{Request,Response}_`. Shared across every node
/// of the image (one descriptor per TYPE, not per node), so the count does not
/// scale with node count.
///
/// Counted here because the model names only what an entry WIRES, and these are
/// wired by the runtime. Before 1268 they registered nothing at all — the six
/// `create_service` calls went straight to the backend, which refused each — so
/// they cost no registry slots and the omission could not be observed.
pub const PARAM_SERVICE_TYPES: usize = 6 * TYPES_PER_SRV;

/// The lifecycle services' types are NOT counted, and that is issue 1293: three
/// of their request types are EMPTY, the descriptor builder refuses an empty
/// schema, and `create_lc_srv` therefore still registers nothing. When 1293
/// gives an empty message its `structure_needs_at_least_one_member` byte in
/// BOTH the descriptor and the serializer, this becomes `5 * TYPES_PER_SRV`
/// minus the shared `GetAvailableTransitions` pair — i.e. 8 — and joins
/// [`infra_types`].
pub const LIFECYCLE_SERVICE_TYPES_WHEN_1293_LANDS: usize = 8;

/// Distinct DDS type names the executor's OWN services add, given what the
/// bringup declares. `param_services` is the caller's answer to
/// `InfraServices::from_model`, so the feature predicate has one spelling.
pub fn infra_types(param_services: bool) -> usize {
    if param_services {
        PARAM_SERVICE_TYPES
    } else {
        0
    }
}

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

/// The `descriptors.cpp` static table's DEMAND for the same registered-type
/// count — phase-454 W6.c, RFC-0100 D5.
///
/// # Why ONE count answers TWO tables
///
/// The two are not independent pools that happen to look alike. `TypeRegistry::
/// get_or_build` (`type_registry.rs`) inserts into its own `FnvIndexMap` and then
/// calls `nros_rmw_cyclonedds_register_descriptor` for the SAME type, so every
/// name that reaches the Rust registry reaches the C++ table on the same line.
/// The C++ table additionally receives the idlc-baked TUs' static-constructor
/// registrations, and those are registrations of types this image compiled a
/// descriptor for — i.e. the same set again, arrived at from the other side.
/// A count that bounds one bounds the other, which is why
/// [`count_dds_types`] + [`infra_types`] is the whole input here as it is for
/// [`derive_max_types`].
///
/// # Why the arithmetic DIFFERS
///
/// [`derive_max_types`] rounds to a power of two because `heapless::FnvIndexMap`
/// requires it, and floors at [`DEFAULT_MAX_TYPES`] so a small entry stays
/// byte-identical. Neither applies to `Entry g_entries[N]`, a plain C array with
/// no capacity constraint of its own — so this publishes the **demand,
/// unfloored** (RFC-0100 D7, issues 1015 + 1033). Zero is a legitimate demand;
/// whether zero is a legal SIZE is decided at the pool, where `descriptors.cpp`
/// keeps its own `#if ... < 1` guard.
///
/// What this replaces is a HAND-AUTHORED 256 whose overflow drops registrations
/// SILENTLY at static-init time and surfaces, much later and nowhere near the
/// cause, as `publisher_create` returning UNSUPPORTED for whichever package
/// happened to be link-order last. The autoware-safety-island workspace
/// registers ~86 types; `std_msgs` + `geometry_msgs` alone are ~60.
pub fn derive_max_descriptor_types(counted: usize) -> usize {
    counted
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

    /// phase-454 W6.c — the descriptor table publishes DEMAND, unfloored.
    ///
    /// The contrast with the test above is the point: the registry's two
    /// adjustments (power of two, floor at the default) are `heapless`'
    /// constraint and a byte-identity promise, and a plain C array has neither.
    /// Floors live at the pool (RFC-0100 D7).
    #[test]
    fn descriptor_demand_is_the_bare_count() {
        assert_eq!(derive_max_descriptor_types(0), 0);
        assert_eq!(derive_max_descriptor_types(1), 1);
        assert_eq!(derive_max_descriptor_types(33), 33, "no power-of-two round");
        assert_eq!(derive_max_descriptor_types(86), 86, "no floor at 256");
    }

    /// The acceptance this wave exists for: a workspace past the hand-authored
    /// 256 cap sizes the table rather than dropping registrations silently.
    ///
    /// Built from the SAME count the registry uses, so this also pins the claim
    /// that one count answers both tables — if the two derivations ever read
    /// different inputs, the relation below stops holding.
    #[test]
    fn an_over_cap_workspace_sizes_the_descriptor_table() {
        // 300 distinct message types -- past `DEFAULT_MAX_DESCRIPTOR_TYPES`,
        // which is where `descriptors.cpp` starts dropping without a word.
        let topics: Vec<(String, String)> = (0..300)
            .map(|i| (format!("/t{i}"), format!("pkg/msg/M{i}")))
            .collect();
        let m = model_with(
            topics
                .iter()
                .map(|(t, ty)| (t.as_str(), ty.as_str(), vec!["/n/e"], vec![]))
                .collect(),
            vec![],
            vec![],
        );
        let counted = count_dds_types(&m, |_| true);
        assert_eq!(counted, 300);
        assert!(
            derive_max_descriptor_types(counted) > DEFAULT_MAX_DESCRIPTOR_TYPES,
            "an image this size must raise the cap, not silently drop 44 types"
        );
        assert_eq!(derive_max_descriptor_types(counted), counted);
        // And the registry's own knob still answers the same question its own
        // way -- one input, two arithmetics.
        assert_eq!(derive_max_types(counted), 512);
    }

    /// Issue 1268 — the six parameter services register twelve types, and the
    /// sizing has to see them: they are the executor's, so no model wiring
    /// names them, and an image whose own types already fill the table would
    /// meet `RegistryFull` at runtime rather than a sized knob at bake time.
    #[test]
    fn infra_types_counts_the_parameter_services_only_when_declared() {
        assert_eq!(infra_types(false), 0, "an image without them pays nothing");
        assert_eq!(
            infra_types(true),
            12,
            "six services x (_Request + _Response); the lifecycle five are issue 1293"
        );
    }
}
