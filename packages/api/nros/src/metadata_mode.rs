//! phase-308 — the process-global recorder that non-Rust adapters feed.
//!
//! The Rust producer needs no global: `record_node_metadata::<C>` owns a
//! `MetadataRecorder` on the stack and hands it to the component as a
//! `NodeContext` sink. A C++ component cannot be driven that way — its
//! `configure(nros::Node&)` reaches the runtime through the `nros_cpp_*` C ABI,
//! and by then there is no Rust value to thread through. So the C/C++ adapter
//! records into a global instead.
//!
//! **The global is the only thing that is new.** It is the SAME
//! [`MetadataRecorder`] type, filled through the same [`push_node`] /
//! [`push_entity`] calls, and dumped through the same
//! [`to_source_metadata_json`] — which is the point. The recorder, the slot
//! accounting and the schema emitter are one implementation; only the adapter
//! that feeds them is per-language. A second recorder would be a second
//! definition of "what is a callback slot", and the count those sidecars carry
//! is the whole reason they exist (issue 0257).
//!
//! [`push_node`]: MetadataRecorder::push_node
//! [`push_entity`]: MetadataRecorder::push_entity
//! [`to_source_metadata_json`]: MetadataRecorder::to_source_metadata_json
//!
//! # Why a global is safe here
//!
//! This module is compiled only under the `metadata-mode` feature, which exists
//! for the host probe: a single-threaded process that constructs one component,
//! runs its declaration path, writes one file and exits. It never ships in
//! firmware — the feature is not reachable from any board or entry build, and
//! enabling it replaces the RMW with a backend that transports nothing.
//!
//! # Node attribution
//!
//! The RMW seam does NOT carry a node: `create_publisher(session, topic_name,
//! type_name, …)` has no participant argument, because by that layer the
//! entity's owning node is already resolved away. A recording backend alone
//! would therefore produce a sidecar whose entities belong to no node — the
//! total count would still be right, but `nodes[]` would be a fiction for any
//! multi-node component.
//!
//! [`begin_node`] closes that: every node entry point — `nros_cpp_node_create*`,
//! `nros-c`'s `nros_executor_node_init` / `nros_node_init_ex`, and the Rust
//! install path's `ExecutorSink::create_node` — calls
//! [`crate::census_hooks::on_node_create`] to open a node and make it current,
//! and every entity recorded afterwards attributes to it.
//! `configure()` declares one node's entities at a time, so a cursor is the
//! correct model — not a heuristic.

use alloc::{string::String, vec::Vec};
// issue 0687 follow-up — the PORTABLE mutex, not `std::sync::Mutex`. This was
// the only thing in the file `alloc` could not supply, and it was keeping
// `metadata-mode` a `std` capability for no reason the code supports: the guard
// beside the feature said "writes a file and exits", but the file write is in
// `nros-cpp`'s `metadata_hooks`, not here.
//
// A spin lock is the right shape rather than a compromise: this module is
// compiled only under `metadata-mode`, whose documented contract (above) is a
// SINGLE-THREADED probe process, so the lock exists to make a global `Sync`,
// not to arbitrate contention that happens. The `spin` edge rides the feature,
// so a build without `metadata-mode` — every firmware image — does not get it.
use nros_rmw::sync::Mutex;

use crate::node_metadata::{
    EntityId, EntityKind, EntityMetadataSpec, MetadataRecorder, NodeId, ParameterDefault,
    SourceMetadataExport, TimerKind, entity_metadata,
};

/// The recorder every non-Rust adapter feeds, plus the current-node cursor.
#[derive(Default)]
struct State {
    recorder: MetadataRecorder,
    /// Node id set by the most recent [`begin_node`]; entities attribute here.
    current_node: Option<String>,
    /// Per-node entity counter, so generated ids are unique and stable.
    seq: usize,
    /// Type names seen so far. The ABI hands them over as borrowed C strings
    /// while `EntityMetadata::type_name` is `&'static str`, so the probe leaks
    /// them deliberately — a process that writes one file and exits.
    leaked: Vec<&'static str>,
}

fn state() -> &'static Mutex<State> {
    static STATE: Mutex<State> = Mutex::new(State {
        recorder: MetadataRecorder::new(),
        current_node: None,
        seq: 0,
        leaked: Vec::new(),
    });
    &STATE
}

fn intern(s: &str) -> &'static str {
    alloc::boxed::Box::leak(String::from(s).into_boxed_str())
}

/// Discard everything recorded so far. For tests; a probe records once.
pub fn reset() {
    state().with(|st| {
        st.recorder = MetadataRecorder::new();
        st.current_node = None;
        st.seq = 0;
    });
}

/// Open a node and make it current. Returns false if the recorder is full —
/// never silently drops, because a dropped node makes every entity after it
/// vanish from the count. A node already open under this name is made current
/// again (issue 1419: a tiered Rust entry creates each node once per tier).
///
/// phase-428 W6 remainder: `#[must_use]` because the doc comment above states
/// the consequence of ignoring the answer and `bool` says nothing at a call
/// site that drops it. RFC-0089 Part I's fourth row — a difference the compiler
/// does not point at must be made loud by other means.
#[must_use]
pub fn begin_node(name: &str, namespace: &str, domain_id: u32) -> bool {
    // phase-457 W3 — arm the registration observer HERE rather than asking every
    // probe entry point to remember. Opening a node is the first thing any
    // adapter does and no entity can be recorded before it, so this is the
    // earliest point that cannot be skipped; a probe that forgot an explicit
    // call would emit rows with the fact silently missing, which reads exactly
    // like the road that legitimately has none.
    listen_for_registrations();
    state().with(|st| {
        let id = String::from(name);
        // Issue 1419 — a node of this name is already open: make it current
        // again. A tiered Rust entry runs a component's `register()` once per
        // tier executor, so the same node is created once per tier, and each
        // tier's run declares only the entities its groups admit. Re-opening
        // is what makes the census their union rather than a panic on tier 2.
        if st.recorder.has_node(&id) {
            st.current_node = Some(id);
            return true;
        }
        if st
            .recorder
            .push_node(NodeId::new(&id), name, namespace, domain_id)
            .is_err()
        {
            return false;
        }
        st.current_node = Some(id);
        true
    })
}

/// Issue 1556 item 1 -- is a node open? An executor-side entity the C API
/// allows before any node (`nros_timer_init` needs only a support context)
/// asks this before it records, so it can open the executor scope instead of
/// failing.
pub fn has_current_node() -> bool {
    state().with(|st| st.current_node.is_some())
}

/// phase-463 W1 -- everything an adapter can say about one entity.
///
/// A struct rather than a seventh positional argument: the two adapters (the
/// recording RMW backend and `nros-cpp`'s executor-side hooks) each fill a
/// different subset, and a call site with five adjacent `Option`s is how the
/// QoS was dropped for two phases without anyone noticing (issue 1419).
#[derive(Debug, Clone, Copy)]
pub struct EntityRecord<'a> {
    pub kind: EntityKind,
    /// The name as the code spelled it (the RMW seam hands over the RESOLVED
    /// name; see `nros-rmw-metadata`).
    pub source_name: &'a str,
    pub type_name: &'a str,
    pub callback_id: Option<&'a str>,
    pub period_ms: Option<u64>,
    /// The profile the entity was created with -- AFTER launch overrides,
    /// because that is the one the executor sizes for. `None` records the
    /// default profile, which is only honest for an entity that has no QoS
    /// (a timer, a guard condition).
    pub qos: Option<crate::QoSProfile>,
    /// Which timer entry point the entity came through; ignored for every
    /// kind but [`EntityKind::Timer`].
    pub timer_kind: TimerKind,
}

impl<'a> EntityRecord<'a> {
    /// A record with only the identifying fields set.
    pub const fn new(kind: EntityKind, source_name: &'a str, type_name: &'a str) -> Self {
        Self {
            kind,
            source_name,
            type_name,
            callback_id: None,
            period_ms: None,
            qos: None,
            timer_kind: TimerKind::Wall,
        }
    }
}

/// Record one entity against the current node.
///
/// `callback_id` and `period_ms` are what distinguish a timer from a
/// subscription in the emitted schema; both are `None` for entities that carry
/// neither. Returns false when there is no current node or the recorder is
/// full.
///
/// phase-428 W6 remainder: see [`begin_node`] — a dropped `false` here is an
/// entity missing from the emitted metadata with nothing said about it.
///
/// phase-463 W1 -- the positional form, kept for the shape every existing
/// caller and test spells; it records the DEFAULT QoS. An adapter that has a
/// QoS to report (every RMW-bound endpoint) goes through [`record`].
#[must_use]
pub fn record_entity(
    kind: EntityKind,
    source_name: &str,
    type_name: &str,
    callback_id: Option<&str>,
    period_ms: Option<u64>,
) -> bool {
    record(EntityRecord {
        callback_id,
        period_ms,
        ..EntityRecord::new(kind, source_name, type_name)
    })
}

/// Record one entity, with everything the adapter knows about it, against the
/// current node. See [`record_entity`] for the refusal contract.
#[must_use]
pub fn record(rec: EntityRecord<'_>) -> bool {
    state().with(|st| {
        let Some(node_id) = st.current_node.clone() else {
            return false;
        };
        let type_name = intern(rec.type_name);
        st.leaked.push(type_name);
        st.seq += 1;
        let id = alloc::format!("{}#{}", rec.source_name, st.seq);
        let spec = EntityMetadataSpec {
            id: EntityId::new(&id),
            node_id: NodeId::new(&node_id),
            kind: rec.kind,
            source_name: rec.source_name,
            type_name,
            type_hash: "",
            qos: rec.qos.unwrap_or_default(),
        };
        let Ok(mut entity) = entity_metadata(spec) else {
            return false;
        };
        entity.period_ms = rec.period_ms;
        entity.timer_kind = rec.timer_kind;
        if let Some(cb) = rec.callback_id {
            entity.callback_id = crate::node_metadata::metadata_string(cb).ok();
        }
        st.recorder.push_entity(entity).is_ok()
    })
}

/// phase-463 W1 -- record a parameter the current node DECLARED, with the
/// type and default the code passed.
///
/// Fed by `nros-cpp`'s `on_param_declare` hook, which sits on the
/// `nros_cpp_node_declare_param_*` family: C++ has no way to declare a
/// parameter that does not cross that ABI, so what lands here is complete by
/// construction. Before this hook existed `parameters: []` in a C++ sidecar
/// meant "nobody looked", not "this node declares none" (issue 1419).
///
/// Same refusal contract as [`record_entity`]: `false` when no node is open or
/// the recorder is full, never a silent drop.
#[must_use]
pub fn record_parameter(name: &str, value: &crate::ParameterValue) -> bool {
    let Ok(default) = ParameterDefault::from_value(value) else {
        return false;
    };
    state().with(|st| {
        let Some(node_id) = st.current_node.clone() else {
            return false;
        };
        st.seq += 1;
        let id = alloc::format!("{}#{}", name, st.seq);
        let spec = EntityMetadataSpec {
            id: EntityId::new(&id),
            node_id: NodeId::new(&node_id),
            kind: EntityKind::Parameter,
            source_name: name,
            type_name: "",
            type_hash: "",
            qos: crate::QoSProfile::default(),
        };
        let Ok(mut entity) = entity_metadata(spec) else {
            return false;
        };
        entity.parameter_type = Some(value.param_type());
        entity.parameter_default = Some(default);
        st.recorder.push_entity(entity).is_ok()
    })
}

/// phase-457 W3 — start hearing what each subscription REGISTRATION says about
/// its own delivery shape.
///
/// Called once by a probe before the component's declaration path runs.
/// Idempotent, and safe to call in a process that registers nothing.
///
/// # Why this is an observation and not a second opinion
///
/// The fact is [`SubscriptionRequest::in_place_capable`][cap] — stated at each
/// of the eleven registration entry points, consumed at the one site that reads
/// `supports_process_in_place`. Nothing outside the executor can derive it: the
/// sizing descriptor used to infer it from the entry's LANGUAGE, which is wrong
/// for nine of the eleven and is issue 0196's class. So the probe RUNS the
/// registration and the registration says.
///
/// A row that nothing observed keeps `None`, and every consumer refuses on it.
/// That is the honest state of the Rust producer's own road, which declares
/// against a recording `NodeContext` and opens no executor at all.
///
/// [cap]: https://docs.rs/nros-node
pub fn listen_for_registrations() {
    nros_node::executor::registration_observer::set_observer(|reg| {
        // A dropped `false` here is a fact silently lost, which is the shape
        // every other recorder entry point is `#[must_use]` against. It is NOT
        // fatal: a registration with no recorded row is a subscription the
        // backend did not record (a second executor, a test harness), and the
        // consumer's refusal is the right outcome — so this reports and carries
        // on rather than panicking inside a registration.
        if !record_subscription_registration(reg.topic, reg.type_name, reg.in_place_capable) {
            nros_log::log_warn!(
                nros_log::get_logger("nros::metadata_mode"),
                "registration of `{}` ({}) matched no recorded subscription row; its \
                 registration path stays refused",
                reg.topic,
                reg.type_name
            );
        }
    });
}

/// Attach one observed registration fact to the subscription row it belongs to.
///
/// See [`MetadataRecorder::observe_subscription_registration`] for the join and
/// its refusal contract.
#[must_use]
pub fn record_subscription_registration(
    source_name: &str,
    type_name: &str,
    in_place_capable: bool,
) -> bool {
    state().with(|st| {
        st.recorder
            .observe_subscription_registration(source_name, type_name, in_place_capable)
    })
}

/// Serialize everything recorded so far, through the one schema emitter.
pub fn to_json(export: &SourceMetadataExport<'_>) -> Result<String, core::fmt::Error> {
    state().with(|st| st.recorder.to_source_metadata_json(export))
}

/// Entities recorded so far — for adapter tests and the probe's own sanity
/// check ("a component that declared nothing is a bug, not an empty sidecar").
pub fn entity_count() -> usize {
    state().with(|st| st.recorder.entities().len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_metadata::EntityKind;

    /// The recorder is process-global by design, so tests that touch it cannot
    /// run concurrently under cargo's thread-per-test harness. Serialize them
    /// rather than making the production type carry a test-only mode.
    // `std::sync::Mutex`, NOT the portable one the module now uses: these tests
    // genuinely contend (cargo runs them on a thread each) and hold the lock
    // across a whole test body, which is exactly the case a spin lock is wrong
    // for. Production never contends — see the module doc.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Issue 1419 -- a tiered Rust entry runs a component's `register()` once
    /// per tier executor, so the SAME node is opened once per tier. Re-opening
    /// makes it current again: one node row, holding the union of what each
    /// tier declared. Red before (measured): the second `begin_node` returned
    /// false, which the hook turns into a panic on tier 2.
    #[test]
    fn reopening_a_node_makes_it_current_again() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(begin_node("ctrl", "/", 0));
        assert!(record_entity(
            EntityKind::Timer,
            "t0",
            "",
            Some("t0"),
            Some(10)
        ));
        assert!(begin_node("telem", "/", 0));
        assert!(
            begin_node("ctrl", "/", 0),
            "a re-opened node is not a failure"
        );
        assert!(record_entity(
            EntityKind::Timer,
            "t1",
            "",
            Some("t1"),
            Some(20)
        ));
        let json = to_json(&SourceMetadataExport::new("p", "c")).expect("serialize");
        assert_eq!(
            json.matches("\"source_default_name\":\"ctrl\"").count(),
            1,
            "one node row, not two: {json}"
        );
        let ctrl = &json[json.find("\"source_default_name\":\"ctrl\"").unwrap()
            ..json.find("\"source_default_name\":\"telem\"").unwrap()];
        assert!(
            ctrl.contains("\"period_ms\":10,") && ctrl.contains("\"period_ms\":20,"),
            "both tiers' timers on the one node: {ctrl}"
        );
        reset();
    }

    /// The C++ shape that motivates phase-307 and 308 together: one
    /// subscription the SystemModel can see, plus timers it cannot. Recorded
    /// through the shared recorder, emitted by the shared serializer.
    ///
    /// Serialized with `language: "cpp"` — which was a hardcoded `"rust"`
    /// literal until this phase, because Rust was the only producer.
    #[test]
    fn records_a_cpp_component_through_the_shared_serializer() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(begin_node("cpp_talker", "/", 0));
        assert!(record_entity(
            EntityKind::Subscription,
            "/chatter",
            "std_msgs/msg/Int32",
            Some("on_chatter"),
            None
        ));
        for i in 0..5 {
            assert!(record_entity(
                EntityKind::Timer,
                &alloc::format!("tick{i}"),
                "",
                Some(&alloc::format!("tick{i}")),
                Some(100)
            ));
        }
        assert_eq!(entity_count(), 6);

        let export = SourceMetadataExport::new("talker_pkg", "cpp_talker")
            .executable("cpp_talker")
            .language("cpp");
        let json = to_json(&export).expect("serialize");
        assert!(json.contains("\"language\":\"cpp\""), "got: {json}");
        assert!(json.contains("\"package\":\"talker_pkg\""), "got: {json}");
        assert!(
            json.contains("\"executable\":\"cpp_talker\""),
            "got: {json}"
        );
        // Five timers, one subscription — the counts the sizing bake reads.
        assert_eq!(json.matches("\"period_ms\"").count(), 5, "got: {json}");
        assert!(json.contains("\"subscribers\":[{"), "got: {json}");
        reset();
    }

    /// issue 0900 — the CLIENT halves reach the sidecar.
    ///
    /// `record_entity` has always accepted `ActionClient`/`ServiceClient`, and
    /// the serializer dropped them: a sidecar described only what a component
    /// SERVES. The executor arena is sized from the client count, so the
    /// omission had a size cost (74,240 vs 16,384 bytes) and not merely a
    /// descriptive one. (phase-392 W6: this used to add "on the task stack",
    /// which was inherited from 0900 and false since phase-271 — the arena is
    /// borrowed from caller-supplied backing, a `.bss` static on every entry
    /// path. The size cost is real; the placement claim was not.)
    #[test]
    fn client_entities_reach_the_sidecar() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(begin_node("client_node", "/", 0));
        assert!(record_entity(
            EntityKind::ActionClient,
            "/fibonacci",
            "example_interfaces/action/Fibonacci",
            None,
            None
        ));
        assert!(record_entity(
            EntityKind::ServiceClient,
            "/add_two_ints",
            "example_interfaces/srv/AddTwoInts",
            None,
            None
        ));
        let export = SourceMetadataExport::new("client_pkg", "client_node").language("rust");
        let json = to_json(&export).expect("serialize");
        assert!(json.contains("\"action_clients\":[{"), "got: {json}");
        assert!(json.contains("\"service_clients\":[{"), "got: {json}");
        // A client registers no callbacks. The server writers emit a
        // `callback`/`goal_callback` field; the client writer must not, which is
        // why it is a separate function rather than a reused one.
        let tail = &json[json.find("\"action_clients\"").expect("array")..];
        let end = tail.find(']').expect("array end");
        assert!(
            !tail[..end].contains("callback"),
            "a client must carry no callback field: {}",
            &tail[..end]
        );
        reset();
    }

    /// phase-463 W1 -- the whole truth reaches the sidecar: the QoS an
    /// endpoint was created with, a timer's kind, a guard condition under its
    /// own kind (still one row of `timers[]`), and a declared parameter with
    /// its type and default.
    ///
    /// Asserted on the SERIALISED form, scoped to the array each fact lives
    /// in, for the reason `client_entities_reach_the_sidecar` gives: the
    /// defects this wave fixes were all in what reached the JSON.
    #[test]
    fn schema_v2_records_qos_timer_kind_guard_and_parameters() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(begin_node("fixture", "/", 0));
        let depth_one = crate::QoSProfile {
            depth: 1,
            ..crate::QoSProfile::default()
        };
        assert!(record(EntityRecord {
            qos: Some(depth_one),
            callback_id: Some("on_cmd"),
            ..EntityRecord::new(
                EntityKind::Subscription,
                "/control/command/control_cmd",
                "autoware_control_msgs/msg/Control",
            )
        }));
        assert!(record(EntityRecord {
            qos: Some(depth_one),
            ..EntityRecord::new(
                EntityKind::ServiceServer,
                "/system/mrm/operate",
                "tier4_system_msgs/srv/OperateMrm",
            )
        }));
        assert!(record(EntityRecord {
            period_ms: Some(250),
            callback_id: Some("once0"),
            timer_kind: TimerKind::Oneshot,
            ..EntityRecord::new(EntityKind::Timer, "once0", "")
        }));
        assert!(record(EntityRecord {
            callback_id: Some("guard0"),
            timer_kind: TimerKind::GuardCondition,
            ..EntityRecord::new(EntityKind::Timer, "guard0", "")
        }));
        assert!(record_parameter(
            "rate",
            &crate::ParameterValue::from_double(30.0)
        ));
        assert!(record_parameter(
            "use_pull_over",
            &crate::ParameterValue::from_bool(false)
        ));
        // Five entities plus two parameters: parameters are entities in the
        // recorder (one slot of its capacity each) and rows of `parameters[]`
        // in the sidecar, never of a node's entity arrays.
        assert_eq!(entity_count(), 6);

        let export = SourceMetadataExport::new("fixture_pkg", "fixture").language("cpp");
        let json = to_json(&export).expect("serialize");
        // READ, not restated -- phase-457 W3 moved the version to 3 and this
        // literal is what noticed. The schema is ADDITIVE, so a v2 fact reaching
        // the sidecar is a claim about the CURRENT version, not about the number
        // 2; pinning the number here would make every additive bump edit a test
        // whose subject did not move.
        assert!(
            json.contains(&alloc::format!(
                "\"version\":{}",
                crate::node_metadata::SOURCE_METADATA_SCHEMA_VERSION
            )),
            "got: {json}"
        );

        let subs = &json[json.find("\"subscribers\":").expect("array")..];
        let subs = &subs[..subs.find(']').unwrap()];
        assert!(
            subs.contains("\"depth\":1"),
            "the QoS the endpoint was created with must reach the row: {subs}"
        );
        let services = &json[json.find("\"services\":").expect("array")..];
        let services = &services[..services.find(']').unwrap()];
        assert!(
            services.contains("\"qos\":{") && services.contains("\"depth\":1"),
            "v2 carries qos on every endpoint kind: {services}"
        );
        let timers = &json[json.find("\"timers\":").expect("array")..];
        let timers = &timers[..timers.find(']').unwrap()];
        assert!(
            timers.contains("\"kind\":\"oneshot\",\"period_ms\":250"),
            "the timer kind sits beside the period: {timers}"
        );
        assert!(
            timers.contains("\"kind\":\"guard_condition\""),
            "a guard condition is a timers[] row under its own kind: {timers}"
        );
        assert_eq!(
            timers.matches("\"kind\":").count(),
            2,
            "one timer, one guard condition, one slot each: {timers}"
        );
        let params = &json[json.find("\"parameters\":").expect("array")..];
        let params = &params[..params.find(']').unwrap()];
        assert!(
            params.contains("\"name\":\"rate\",\"type\":\"double\",\"default\":30.0"),
            "the declared type and the code default: {params}"
        );
        assert!(
            params.contains("\"name\":\"use_pull_over\",\"type\":\"bool\",\"default\":false"),
            "got: {params}"
        );
        reset();
    }

    /// A parameter recorded with no open node is refused like any entity.
    #[test]
    fn parameters_without_a_node_are_refused() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(!record_parameter(
            "rate",
            &crate::ParameterValue::from_double(30.0)
        ));
        assert_eq!(entity_count(), 0);
        reset();
    }

    /// An entity recorded with no open node is a bug in the adapter, and must
    /// be refused rather than silently attributed or dropped — a dropped entity
    /// is an under-sized executor at boot.
    #[test]
    fn entities_without_a_node_are_refused() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(!record_entity(
            EntityKind::Timer,
            "tick",
            "",
            Some("tick"),
            Some(10)
        ));
        assert_eq!(entity_count(), 0);
        reset();
    }

    /// phase-457 W3 — a subscription nothing observed carries NO registration
    /// fact, and the sidecar says so by omitting the key.
    ///
    /// The whole point: an absent `in_place` is a refusal, and a consumer that
    /// read it as `false` would state a registration path nobody measured. This
    /// is also the state of every row the RUST producer writes, which declares
    /// against a recording `NodeContext` and opens no executor.
    #[test]
    fn an_unobserved_subscription_carries_no_registration_fact() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(begin_node("fixture", "/", 0));
        assert!(record(EntityRecord {
            callback_id: Some("on_chatter"),
            ..EntityRecord::new(EntityKind::Subscription, "/chatter", "std_msgs/msg/String",)
        }));
        let export = SourceMetadataExport::new("fixture_pkg", "fixture").language("rust");
        let json = to_json(&export).expect("serialize");
        assert!(
            !json.contains("\"in_place\""),
            "an unobserved row must omit the key, not state a false: {json}"
        );
        reset();
    }

    /// The observation lands on the row that registered it, and the two
    /// directions are both carried.
    ///
    /// Two subscriptions of the same type on different topics, one from a
    /// borrowed-bytes delivery shape and one from a shape that needs more — the
    /// exact pair that makes an image-wide answer wrong.
    #[test]
    fn an_observed_registration_lands_on_its_own_row() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(begin_node("fixture", "/", 0));
        for topic in ["/plain", "/with_info"] {
            assert!(record(EntityRecord {
                callback_id: Some("cb"),
                ..EntityRecord::new(EntityKind::Subscription, topic, "std_msgs/msg/String")
            }));
        }
        assert!(record_subscription_registration(
            "/plain",
            "std_msgs/msg/String",
            true
        ));
        assert!(record_subscription_registration(
            "/with_info",
            "std_msgs/msg/String",
            false
        ));
        let export = SourceMetadataExport::new("fixture_pkg", "fixture").language("cpp");
        let json = to_json(&export).expect("serialize");
        let plain = json.find("/plain").expect("row");
        let info = json.find("/with_info").expect("row");
        assert!(
            json[plain..info].contains("\"in_place\":true"),
            "got: {json}"
        );
        assert!(json[info..].contains("\"in_place\":false"), "got: {json}");
        reset();
    }

    /// A second observation does NOT restate the first: it lands on the next
    /// unset row, and an observation matching no waiting row is REFUSED.
    ///
    /// Two subscriptions indistinguishable at this seam — same topic, same type
    /// — are attributed in registration order, which is the only attribution
    /// available. Overwriting would make two disagreeing rows read as one, and
    /// disagreement is the case the field exists for.
    #[test]
    fn observations_fill_rows_in_order_and_refuse_when_none_waits() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(begin_node("fixture", "/", 0));
        for _ in 0..2 {
            assert!(record(EntityRecord {
                callback_id: Some("cb"),
                ..EntityRecord::new(EntityKind::Subscription, "/dup", "std_msgs/msg/String")
            }));
        }
        assert!(record_subscription_registration(
            "/dup",
            "std_msgs/msg/String",
            true
        ));
        assert!(record_subscription_registration(
            "/dup",
            "std_msgs/msg/String",
            false
        ));
        // A third has no row left to land on.
        assert!(!record_subscription_registration(
            "/dup",
            "std_msgs/msg/String",
            true
        ));
        // And a topic nobody recorded is refused rather than attributed to a
        // neighbour.
        assert!(!record_subscription_registration(
            "/never",
            "std_msgs/msg/String",
            true
        ));
        let export = SourceMetadataExport::new("fixture_pkg", "fixture").language("cpp");
        let json = to_json(&export).expect("serialize");
        assert!(json.contains("\"in_place\":true"), "got: {json}");
        assert!(json.contains("\"in_place\":false"), "got: {json}");
        reset();
    }
}
