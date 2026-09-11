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
//! * **durability** — VOLATILE is what a shim with no historical-sample cache
//!   does; TRANSIENT_LOCAL is refused (it was already outside the mask, this
//!   makes the refusal local and loud for a direct caller).
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

use nros_rmw::{
    DURATION_INFINITE_MS, QoSDurabilityPolicy, QoSHistoryPolicy, QoSLivelinessPolicy, QoSProfile,
    QoSReliabilityPolicy, TransportError,
};

use crate::config::SUBSCRIBER_RING_DEPTH;

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

    /// How many samples the entity's receive ring can hold, or `None` when the
    /// entity has no ring.
    ///
    /// A publisher has none: zenoh-pico writes to the session on the calling
    /// thread and queues no sample of its own, so there is no history for a
    /// depth to bound and no sample a depth could cause it to drop. Any
    /// requested depth is therefore satisfied — vacuously, but exactly.
    fn ring_capacity(self) -> Option<u32> {
        match self {
            EntityKind::Publisher => None,
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
static DEPTH_CLAMP_REPORTED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Reported once per process, same reasoning: `QOS_PROFILE_SENSOR_DATA` and
/// `QOS_PROFILE_BEST_EFFORT` are common, and the answer is the same every time.
static RELIABILITY_GRANT_REPORTED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

fn first_time(flag: &core::sync::atomic::AtomicBool) -> bool {
    !flag.swap(true, core::sync::atomic::Ordering::Relaxed)
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

    // nros-qos-honours: DURABILITY_VOLATILE — no historical-sample cache and no
    // query-on-match, so VOLATILE is what the shim does and TRANSIENT_LOCAL is
    // refused. The mask already withholds TRANSIENT_LOCAL, so the runtime
    // refuses it first; this is the same refusal for a caller who reached the
    // backend directly.
    match requested.durability {
        QoSDurabilityPolicy::Volatile => {}
        QoSDurabilityPolicy::TransientLocal => {
            refuse(
                kind,
                name,
                "durability",
                "TRANSIENT_LOCAL — the shim keeps no historical samples",
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
    if let Some(capacity) = kind.ring_capacity() {
        if requested.depth > capacity {
            granted.depth = capacity;
            if first_time(&DEPTH_CLAMP_REPORTED) {
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

    #[test]
    fn transient_local_is_refused() {
        let mut qos = base();
        qos.durability = QoSDurabilityPolicy::TransientLocal;
        assert!(matches!(
            admit(EntityKind::Subscription, "/t", &qos),
            Err(TransportError::IncompatibleQos)
        ));
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
