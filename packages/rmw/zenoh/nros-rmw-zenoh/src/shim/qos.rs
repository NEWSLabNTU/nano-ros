//! QoS admission — what this backend GRANTS for what a caller requested.
//!
//! phase-428 W9. Every bit `ZenohSession::supported_qos_policies` sets is
//! backed by a `nros-qos-honours:` claim, and a claim means the backend READS
//! the field and either applies it or refuses a value it cannot serve.
//! `check-qos-mask-derivation` re-measures that on every push, so the mask is
//! derived from this file rather than asserted beside it.
//!
//! # What W9 measured here, and what changed
//!
//! Before this module the shim read four of its own CORE policies in exactly
//! one place — `QosKeyExpr::to_qos_string`, the DISCOVERY keyexpr — and read
//! them nowhere else. Reliability, durability, history and depth went onto the
//! `@ros2_lv` token and changed no local behaviour whatever. A caller asking
//! for `KEEP_LAST(100)` got a four-slot ring AND a graph entry telling every
//! peer it had a hundred.
//!
//! Three of those four are now honoured for real, by refusing what the shim
//! cannot do:
//!
//! * **history** — the ring is KEEP_LAST by construction (`subscriber.rs`
//!   drops on a full ring), so `KEEP_ALL` is refused rather than silently
//!   served as KEEP_LAST. Nothing in the tree requests it: no named preset is
//!   KEEP_ALL since phase-428 W10.
//! * **durability** — VOLATILE is what an entity with no retention does.
//!   TRANSIENT_LOCAL is SERVED on a publisher since phase-455 W5 and refused on
//!   every other kind.
//! * **depth** — clamped to the ring, which is a build-time constant, and the
//!   clamp is REPORTED: the granted value goes into the graph token, and the
//!   first entity to lose depth says so once per session.
//!
//! The fourth, **reliability**, is granted RELIABLE whatever was asked,
//! because zenoh-pico's publisher path sets `Z_CONGESTION_CONTROL_BLOCK`
//! unconditionally. That is an OVER-delivery — a BEST_EFFORT reader is
//! RxO-compatible with a RELIABLE writer, so it costs a peer nothing — and it
//! is reported at INFO where the depth clamp is reported at WARN. The severity
//! IS the direction: over-delivery is safe, under-delivery loses samples.
//!
//! # Why granting is not clamping
//!
//! A clamp changes what happens and tells nobody. This returns the profile the
//! backend will actually run, the caller's entity is configured from it, and
//! `create_*` puts THAT profile — not the request — into the liveliness token
//! a `rmw_zenoh_cpp` peer parses. So the graph stops carrying a number we do
//! not honour, which was the lie.
//!
//! # Why refusing was not free either — phase-455 W5, issue 1341
//!
//! W9's rule is right and the population it was applied to was not.
//! `QOS_PROFILE_ACTION_STATUS_DEFAULT` mirrors
//! `rcl_action_qos_profile_status_default` and is the action protocol's WIRE
//! CONTRACT, not a caller's preference this backend may decline: a client that
//! joins late, or is slow, learns a goal's terminal state only because
//! `/status` is transient-local. Refusing it stopped every zenoh action server
//! from starting; granting VOLATILE instead would have kept them starting and
//! broken RxO against a stock `rclcpp` client, whose `/status` reader is
//! transient-local — the "goal terminated, client never saw it" failure issue
//! 0902 exists for.
//!
//! So the publisher SERVES it, by the mechanism the paragraph above already
//! named as the missing one: query-on-match. What a transient-local publisher
//! retains is `TL_RETAIN_DEPTH` sample, and `queue_capacity` makes that the
//! depth the graph advertises, so the advertised durability and the advertised
//! depth are both the served ones.

use nros_rmw::{
    DURATION_INFINITE_MS, QoSDurabilityPolicy, QoSHistoryPolicy, QoSLivelinessPolicy, QoSProfile,
    QoSReliabilityPolicy, TransportError,
};

use portable_atomic::Ordering;

use crate::config::{SUBSCRIBER_RING_DEPTH, TL_RETAIN_DEPTH};

use super::service::SERVICE_REQUEST_RING_DEPTH;

/// Which create entry is asking. The granted profile differs by entity: the
/// queue a depth names is a different array for a subscription and a service
/// server, and a publisher has none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntityKind {
    Publisher,
    Subscription,
    Service,
    Client,
}

impl EntityKind {
    fn label(self) -> &'static str {
        match self {
            EntityKind::Publisher => "publisher",
            EntityKind::Subscription => "subscription",
            EntityKind::Service => "service",
            EntityKind::Client => "client",
        }
    }

    /// How many samples the entity's queue can hold for the requested
    /// durability, or `None` when the entity has no queue.
    ///
    /// A VOLATILE publisher has none: zenoh-pico writes to the session on the
    /// calling thread and queues no sample of its own, so there is no history
    /// for a depth to bound and no sample a depth could cause it to drop. Any
    /// requested depth is therefore satisfied — vacuously, but exactly.
    ///
    /// phase-455 W5 — a TRANSIENT_LOCAL publisher is the exception, and it is
    /// not vacuous: it RETAINS, and `TL_RETAIN_DEPTH` samples is what it
    /// retains. The depth a transient-local publisher advertises has to be the
    /// number a late joiner will actually receive, so this is the one place a
    /// publisher's depth meets a real queue. `rcl_action_qos_profile_status_
    /// default` is KEEP_LAST(1), so the motivating caller is served exactly and
    /// anything deeper is reported by the clamp below rather than pocketed.
    fn queue_capacity(self, durability: QoSDurabilityPolicy) -> Option<u32> {
        match self {
            EntityKind::Publisher => match durability {
                QoSDurabilityPolicy::TransientLocal => Some(TL_RETAIN_DEPTH),
                _ => None,
            },
            EntityKind::Subscription => Some(SUBSCRIBER_RING_DEPTH as u32),
            EntityKind::Service | EntityKind::Client => Some(SERVICE_REQUEST_RING_DEPTH as u32),
        }
    }
}

/// Reported once per process, not once per entity.
///
/// Every stock preset asks for more depth than the default four-slot ring
/// (`QOS_PROFILE_DEFAULT` and `QOS_PROFILE_SERVICES_DEFAULT` ask 10,
/// `QOS_PROFILE_PARAMETERS` asks 1000), so a line per entity would be a line
/// per entity in every image — noise that teaches a reader to skip it. The
/// knob that fixes it is global, so one line naming the first entity to lose
/// depth carries the whole message.
///
/// issue 1344 — `portable_atomic`, never `core::sync::atomic`. `riscv32imc`
/// (every esp32 image) has no A extension, so core's `AtomicBool` there is
/// load/store only and has no `swap` at all: the latch below compiles on the
/// host and fails the whole crate on that target. `portable-atomic` is already
/// a dependency and already the spelling one module up (`shim/mod.rs:49`), and
/// it resolves a CAS-less target through the `critical-section` /
/// `unsafe-assume-single-core` feature the consuming board enables.
static DEPTH_CLAMP_REPORTED: portable_atomic::AtomicBool = portable_atomic::AtomicBool::new(false);

/// Reported once per process, same reasoning: `QOS_PROFILE_SENSOR_DATA` and
/// `QOS_PROFILE_BEST_EFFORT` are common, and the answer is the same every time.
static RELIABILITY_GRANT_REPORTED: portable_atomic::AtomicBool =
    portable_atomic::AtomicBool::new(false);

/// phase-455 W5 — the transient-local publisher's depth clamp, latched
/// SEPARATELY from the receive ring's. See the clamp site for why sharing one
/// latch would silence this one on every image that has a transient-local
/// publisher at all.
static TL_DEPTH_CLAMP_REPORTED: portable_atomic::AtomicBool =
    portable_atomic::AtomicBool::new(false);

fn first_time(flag: &portable_atomic::AtomicBool) -> bool {
    !flag.swap(true, Ordering::Relaxed)
}

/// Resolve a requested profile into the one this backend will run, refusing
/// what it cannot serve.
///
/// The caller must have resolved `SYSTEM_DEFAULT` first (issue 0829 —
/// `QoSProfile::resolve_system_default`); a sentinel reaching here would be
/// compared against a ring capacity as if it were a request.
pub(super) fn admit(
    kind: EntityKind,
    name: &str,
    requested: &QoSProfile,
) -> Result<QoSProfile, TransportError> {
    let mut granted = *requested;

    // nros-qos-honours: HISTORY — the ring is KEEP_LAST by construction
    // (`subscriber.rs` drops the arriving sample when `tail - head` reaches the
    // ring depth), so KEEP_ALL is refused rather than served as its opposite.
    // KEEP_ALL blocks a reliable writer where KEEP_LAST overwrites: serving one
    // as the other inverts behaviour, it does not weaken it.
    match requested.history {
        QoSHistoryPolicy::KeepLast => {}
        QoSHistoryPolicy::KeepAll => {
            refuse(
                kind,
                name,
                "history",
                "KEEP_ALL — the receive ring is KEEP_LAST only",
            );
            return Err(TransportError::IncompatibleQos);
        }
        // A sentinel HERE means the caller skipped `resolve_system_default`,
        // which is a caller bug and not a policy this backend declines. Saying
        // "KEEP_ALL" would aim the reader at their profile instead of at the
        // missing resolve.
        QoSHistoryPolicy::SystemDefault => {
            refuse(
                kind,
                name,
                "history",
                "SYSTEM_DEFAULT reached the backend unresolved",
            );
            return Err(TransportError::IncompatibleQos);
        }
    }

    // nros-qos-honours: DURABILITY_VOLATILE — VOLATILE is served by every
    // entity kind, which is what an entity with no retention does.
    //
    // nros-qos-honours: DURABILITY_TRANSIENT_LOCAL — served by a PUBLISHER,
    // through query-on-match (`shim/publisher.rs::transient_local`): the
    // publisher retains its last `TL_RETAIN_DEPTH` sample and declares a
    // queryable on `<keyexpr>/@adv/pub/<zid>/<eid>/_`, which is where a stock
    // `ze_advanced_subscriber`'s history query lands. Refused for every other
    // kind — the SUBSCRIBER half (querying a stock transient-local publisher on
    // match, for a latched topic) is a real capability and is not built.
    //
    // phase-455 W5 / issue 1341 — this arm refused ALL FOUR kinds from
    // phase-428 W9 (2026-09-12) until now, and the one profile in the tree whose
    // durability is not VOLATILE is the one an action server creates:
    // `QOS_PROFILE_ACTION_STATUS_DEFAULT` mirrors
    // `rcl_action_qos_profile_status_default`, so a zenoh action server could
    // not start at all. That profile is the action protocol's wire contract and
    // not a caller request this backend may decline, and granting VOLATILE
    // instead would break RxO against a stock `rclcpp` client — whose `/status`
    // reader IS transient-local — while reintroducing by design the
    // "goal terminated, client never saw it" failure of issue 0902.
    match requested.durability {
        QoSDurabilityPolicy::Volatile => {}
        QoSDurabilityPolicy::TransientLocal if matches!(kind, EntityKind::Publisher) => {}
        QoSDurabilityPolicy::TransientLocal => {
            refuse(
                kind,
                name,
                "durability",
                "TRANSIENT_LOCAL — the shim serves publisher-side retention only; \
                 a subscription cannot query a peer's cache on match yet",
            );
            return Err(TransportError::IncompatibleQos);
        }
        // See the history arm: an unresolved sentinel is the caller's bug.
        QoSDurabilityPolicy::SystemDefault => {
            refuse(
                kind,
                name,
                "durability",
                "SYSTEM_DEFAULT reached the backend unresolved",
            );
            return Err(TransportError::IncompatibleQos);
        }
    }

    // nros-qos-honours: RELIABILITY — read, granted RELIABLE, and the
    // divergence reported. zenoh-pico publishes with
    // `Z_CONGESTION_CONTROL_BLOCK`, so a BEST_EFFORT request is OVER-served,
    // never under-served. Reported at INFO once: a BEST_EFFORT reader is
    // RxO-compatible with a RELIABLE writer, so this costs a peer nothing.
    if requested.reliability != QoSReliabilityPolicy::Reliable {
        granted.reliability = QoSReliabilityPolicy::Reliable;
        if first_time(&RELIABILITY_GRANT_REPORTED) {
            nros_log::log_info!(
                nros_log::get_logger("nros_rmw_zenoh"),
                "qos: {} '{}' asked for BEST_EFFORT; zenoh-pico delivers reliably \
                 (congestion control BLOCK), so RELIABLE is granted. Over-delivery, \
                 not loss.",
                kind.label(),
                name
            );
        }
    }

    // nros-qos-honours: DEPTH — the requested depth meets the ring the shim
    // actually has. The ring is a build-time constant
    // (`ZPICO_SUBSCRIBER_RING_DEPTH`, default 4), so a bigger request cannot be
    // served; what it must not do is vanish. The granted depth is what
    // `create_*` puts in the liveliness token, so a peer reading our graph
    // entry sees the queue we keep, and the first loss says so once.
    if let Some(capacity) = kind.queue_capacity(requested.durability) {
        if requested.depth > capacity {
            granted.depth = capacity;
            // phase-455 W5 — the transient-local publisher's clamp names its
            // own bound, because `ZPICO_SUBSCRIBER_RING_DEPTH` is not the knob
            // that would fix it and `TL_RETAIN_DEPTH` is not a knob at all. A
            // message naming the wrong knob is worse than no message: it sends
            // the reader to change a number that changes nothing.
            //
            // And it takes its OWN once-per-process latch rather than sharing
            // the ring's. They are different facts with different remedies, and
            // sharing one latch means whichever entity is created first
            // silences the other — measured: every action server clamps its
            // `send_goal` service ring (10 -> 4) before it creates the `/status`
            // publisher, so a shared latch would make the transient-local
            // downgrade permanently invisible on exactly the image it matters on.
            let transient_local_publisher = matches!(kind, EntityKind::Publisher);
            let latch = if transient_local_publisher {
                &TL_DEPTH_CLAMP_REPORTED
            } else {
                &DEPTH_CLAMP_REPORTED
            };
            if first_time(latch) {
                if transient_local_publisher {
                    nros_log::log_warn!(
                        nros_log::get_logger("nros_rmw_zenoh"),
                        "qos: publisher '{}' asked for TRANSIENT_LOCAL KEEP_LAST({}); \
                         this backend retains {} sample and replays it on a late \
                         joiner's query. Granting {} and advertising it to the graph.",
                        name,
                        requested.depth,
                        capacity,
                        capacity
                    );
                } else {
                    nros_log::log_warn!(
                        nros_log::get_logger("nros_rmw_zenoh"),
                        "qos: {} '{}' asked for KEEP_LAST({}); this image's receive ring \
                         holds {}. Granting {} and advertising it to the graph. Raise \
                         ZPICO_SUBSCRIBER_RING_DEPTH to keep more.",
                        kind.label(),
                        name,
                        requested.depth,
                        capacity,
                        capacity
                    );
                }
            }
        }
    }

    // nros-qos-honours: LIVELINESS_AUTOMATIC — the session declares an
    // `@ros2_lv` token for the entity at create and holds it for the entity's
    // life, which IS automatic assertion: the middleware asserts, the
    // application does not. Read here so the claim has a site; nothing to
    // apply, because unconditional is what AUTOMATIC asks for.
    //
    // nros-qos-honours: LIVELINESS_MANUAL_BY_TOPIC — `publisher.rs` runs the
    // keepalive timer and fires `LivelinessLost` when the app misses a lease.
    //
    // MANUAL_BY_NODE is REFUSED, and was withdrawn from the mask in phase-428
    // W9. The shim asserts per PUBLISHER; per-node semantics say one assertion
    // covers every publisher of the node, so serving it as by-topic expires
    // publishers the application correctly believed it had kept alive. That is
    // a lie in the losing direction, which is why it is not a downgrade we are
    // willing to make silently. (Cyclone folds the same value onto
    // MANUAL_BY_TOPIC in `qos.cpp` — issue 1328.)
    match requested.liveliness_kind {
        QoSLivelinessPolicy::None
        | QoSLivelinessPolicy::Automatic
        | QoSLivelinessPolicy::ManualByTopic => {}
        QoSLivelinessPolicy::ManualByNode => {
            refuse(
                kind,
                name,
                "liveliness",
                "MANUAL_BY_NODE — the shim asserts per publisher, not per node",
            );
            return Err(TransportError::IncompatibleQos);
        }
    }

    // nros-qos-honours: LIVELINESS_LEASE — `publisher.rs` gates and rate-limits
    // `LivelinessLost` on it. The SUBSCRIBER side does not: its alive-state
    // poll runs at a fixed interval (`LIVELINESS_POLL_DEFAULT_MS`), so a lease
    // stated on a subscription buys nothing and is reported rather than
    // pocketed.
    if matches!(kind, EntityKind::Subscription)
        && requested.liveliness_lease_ms != 0
        && requested.liveliness_lease_ms != DURATION_INFINITE_MS
    {
        nros_log::log_warn!(
            nros_log::get_logger("nros_rmw_zenoh"),
            "qos: subscription '{}' stated a {} ms liveliness lease; the shim's \
             alive-state poll runs at a fixed interval and does not use it. \
             Publisher-side leases ARE honoured.",
            name,
            requested.liveliness_lease_ms
        );
    }

    // deadline_ms and lifespan_ms need no admission step: both are applied
    // verbatim by `publisher.rs` / `subscriber.rs`, and both take 0 and
    // DURATION_INFINITE_MS as "off". Their `nros-qos-honours:` claims are sited
    // on the code that checks them, which is where the honouring happens.

    Ok(granted)
}

fn refuse(kind: EntityKind, name: &str, policy: &str, why: &str) {
    nros_log::log_error!(
        nros_log::get_logger("nros_rmw_zenoh"),
        "qos: {} '{}' refused — {} {}",
        kind.label(),
        name,
        policy,
        why
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> QoSProfile {
        QoSProfile::QOS_PROFILE_DEFAULT
    }

    #[test]
    fn a_depth_the_ring_cannot_hold_is_granted_down_and_the_grant_is_returned() {
        let mut qos = base();
        qos.depth = 10_000;
        let granted = admit(EntityKind::Subscription, "/t", &qos).expect("admissible");
        assert_eq!(granted.depth, SUBSCRIBER_RING_DEPTH as u32);
        // The REQUEST is untouched — the grant is a separate value, so a caller
        // can see both.
        assert_eq!(qos.depth, 10_000);
    }

    #[test]
    fn a_depth_the_ring_can_hold_is_granted_verbatim() {
        let mut qos = base();
        qos.depth = 1;
        let granted = admit(EntityKind::Subscription, "/t", &qos).expect("admissible");
        assert_eq!(granted.depth, 1);
    }

    #[test]
    fn a_publisher_has_no_ring_so_any_depth_is_granted() {
        let mut qos = base();
        qos.depth = 65_535;
        let granted = admit(EntityKind::Publisher, "/t", &qos).expect("admissible");
        assert_eq!(granted.depth, 65_535);
    }

    #[test]
    fn keep_all_is_refused_not_served_as_keep_last() {
        let mut qos = base();
        qos.history = QoSHistoryPolicy::KeepAll;
        assert!(matches!(
            admit(EntityKind::Subscription, "/t", &qos),
            Err(TransportError::IncompatibleQos)
        ));
    }

    /// phase-455 W5 / issue 1341 — the split is by ENTITY KIND, because the
    /// publisher half is built and the subscriber half is not.
    #[test]
    fn transient_local_is_served_on_a_publisher_and_refused_on_every_other_kind() {
        let mut qos = base();
        qos.durability = QoSDurabilityPolicy::TransientLocal;
        let granted = admit(EntityKind::Publisher, "/t", &qos).expect("the publisher serves it");
        assert_eq!(granted.durability, QoSDurabilityPolicy::TransientLocal);
        for kind in [
            EntityKind::Subscription,
            EntityKind::Service,
            EntityKind::Client,
        ] {
            assert!(
                matches!(
                    admit(kind, "/t", &qos),
                    Err(TransportError::IncompatibleQos)
                ),
                "{} must refuse TRANSIENT_LOCAL until the subscriber half exists",
                kind.label()
            );
        }
    }

    /// The advertised depth is the RETAINED depth, which is the whole reason a
    /// transient-local publisher has a queue capacity at all. A publisher that
    /// asked for more must not advertise more — that is the lie phase-428 W9
    /// removed, and re-adding it here would be the same defect one policy over.
    #[test]
    fn a_transient_local_publisher_advertises_the_depth_it_retains() {
        let mut qos = base();
        qos.durability = QoSDurabilityPolicy::TransientLocal;
        qos.depth = 10;
        let granted = admit(EntityKind::Publisher, "/t", &qos).expect("admissible");
        assert_eq!(granted.depth, crate::config::TL_RETAIN_DEPTH);
        assert_eq!(granted.durability, QoSDurabilityPolicy::TransientLocal);

        // and a VOLATILE publisher still has no queue, so its depth is granted
        // verbatim — the two arms must not converge.
        qos.durability = QoSDurabilityPolicy::Volatile;
        qos.depth = 10;
        assert_eq!(
            admit(EntityKind::Publisher, "/t", &qos)
                .expect("admissible")
                .depth,
            10
        );
    }

    /// The profile issue 1341 is about, end to end through admission.
    #[test]
    fn the_action_status_profile_survives_admission_on_a_publisher() {
        let qos = QoSProfile::QOS_PROFILE_ACTION_STATUS_DEFAULT;
        assert_eq!(qos.durability, QoSDurabilityPolicy::TransientLocal);
        let granted = admit(EntityKind::Publisher, "/fibonacci/_action/status", &qos)
            .expect("an action server must be able to create its status publisher");
        assert_eq!(granted.durability, QoSDurabilityPolicy::TransientLocal);
        assert_eq!(
            granted.depth, qos.depth,
            "KEEP_LAST(1) is exactly what the retention serves, so nothing is clamped"
        );
    }

    #[test]
    fn manual_by_node_is_refused_and_manual_by_topic_is_not() {
        let mut qos = base();
        qos.liveliness_kind = QoSLivelinessPolicy::ManualByNode;
        assert!(matches!(
            admit(EntityKind::Publisher, "/t", &qos),
            Err(TransportError::IncompatibleQos)
        ));
        qos.liveliness_kind = QoSLivelinessPolicy::ManualByTopic;
        assert!(admit(EntityKind::Publisher, "/t", &qos).is_ok());
    }

    #[test]
    fn best_effort_is_granted_reliable_rather_than_refused() {
        let mut qos = base();
        qos.reliability = QoSReliabilityPolicy::BestEffort;
        let granted = admit(EntityKind::Publisher, "/t", &qos).expect("admissible");
        assert_eq!(granted.reliability, QoSReliabilityPolicy::Reliable);
    }

    /// Every stock preset a default caller reaches must survive admission —
    /// this is the regression the roadmap's "nothing that works today breaks"
    /// claim needed and did not have.
    #[test]
    fn every_stock_preset_a_default_caller_reaches_is_admissible() {
        let defaults = nros_rmw::QoSSystemDefaults {
            reliability: QoSReliabilityPolicy::Reliable,
            durability: QoSDurabilityPolicy::Volatile,
            history: QoSHistoryPolicy::KeepLast,
            depth: SUBSCRIBER_RING_DEPTH as u32,
        };
        for (label, qos) in [
            ("default", QoSProfile::QOS_PROFILE_DEFAULT),
            ("services", QoSProfile::QOS_PROFILE_SERVICES_DEFAULT),
            ("sensor", QoSProfile::QOS_PROFILE_SENSOR_DATA),
            ("parameters", QoSProfile::QOS_PROFILE_PARAMETERS),
            ("parameter_events", QoSProfile::QOS_PROFILE_PARAMETER_EVENTS),
            ("system_default", QoSProfile::QOS_PROFILE_SYSTEM_DEFAULT),
            ("best_effort", QoSProfile::BEST_EFFORT),
            ("reliable", QoSProfile::RELIABLE),
        ] {
            let qos = qos.resolve_system_default(&defaults);
            for kind in [
                EntityKind::Publisher,
                EntityKind::Subscription,
                EntityKind::Service,
                EntityKind::Client,
            ] {
                assert!(
                    admit(kind, "/t", &qos).is_ok(),
                    "{label} refused for a {}",
                    kind.label()
                );
            }
        }
    }
}
