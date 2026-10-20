//! Issue 1978 — which status events this backend can carry, stated ONCE.
//!
//! Three questions read this answer: the per-entity
//! `Publisher::supports_event` and `Subscription::supports_event`, and the
//! backend-level `Session::event_type_is_supported` (upstream Jazzy
//! `rmw_event_type_is_supported`, reached through the C-ABI adapter). Each
//! used to be its own `matches!`; one table means the backend-level answer
//! can never deny a kind an entity claims, or claim one no entity carries.

use nros_rmw::EventKind;

/// Publisher-side kinds: `OfferedDeadlineMissed` (clock-based check on
/// `publish_raw`) and `LivelinessLost` (the manual-liveliness watchdog).
pub(crate) const fn publisher_event_supported(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::OfferedDeadlineMissed | EventKind::LivelinessLost
    )
}

/// Subscription-side kinds: `MessageLost` (attachment sequence gap),
/// `RequestedDeadlineMissed` (clock-based poll), and — off bare metal, where
/// the session-side liveliness subscriber exists — `LivelinessChanged`.
pub(crate) const fn subscription_event_supported(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::MessageLost | EventKind::RequestedDeadlineMissed
    ) || (matches!(kind, EventKind::LivelinessChanged)
        && cfg!(not(feature = "platform-bare-metal")))
}

/// The backend-level answer: either side carries it.
pub(crate) const fn event_type_is_supported(kind: EventKind) -> bool {
    publisher_event_supported(kind) || subscription_event_supported(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_answer_is_the_union_of_the_entity_answers() {
        for kind in [
            EventKind::LivelinessChanged,
            EventKind::RequestedDeadlineMissed,
            EventKind::MessageLost,
            EventKind::LivelinessLost,
            EventKind::OfferedDeadlineMissed,
        ] {
            assert_eq!(
                event_type_is_supported(kind),
                publisher_event_supported(kind) || subscription_event_supported(kind),
            );
        }
        assert!(event_type_is_supported(EventKind::OfferedDeadlineMissed));
        assert!(event_type_is_supported(EventKind::MessageLost));
    }
}
