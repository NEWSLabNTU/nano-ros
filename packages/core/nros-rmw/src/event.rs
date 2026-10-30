//! Status-event surface (Phase 108).
//!
//! Tier-1 events that backends optionally surface — liveliness
//! changes, deadline misses, message loss. Dispatched via
//! callback-on-entity (registration on `Subscription` / `Publisher`),
//! not via upstream's waitset-take pattern.
//!
//! Events fire from inside [`Session::drive_io`](crate::Session::drive_io)
//! on the executor thread, the same as message callbacks. (Phase 105 counted
//! both against a per-spin cap, ~~`max_callbacks_per_spin`~~; the executor has
//! no such cap now.)
//!
//! See `book/src/concepts/status-events.md` for the user-facing
//! patterns and `book/src/design/rmw-vs-upstream.md` Section 8 for
//! the design rationale.
//!
//! ### Matched and incompatible-type (issue 1972)
//!
//! Iron/Jazzy's `*_MATCHED` and `*_INCOMPATIBLE_TYPE` kinds are appended
//! after the Tier-1 set ([`EventKind::PublicationMatched`] onward), with
//! payloads [`MatchedStatus`] and [`IncompatibleTypeStatus`]. Upstream's
//! QOS_INCOMPATIBLE is still surfaced synchronously at create time as
//! `TransportError::IncompatibleQos` (`NROS_RMW_RET_INCOMPATIBLE_QOS` at the
//! C boundary); no runtime event for it.

/// Status-event kinds. `#[non_exhaustive]` so adding a variant later is not
/// a break for matchers. Discriminants equal the C `rmw_event_type_t`
/// values, which only ever grow at the end.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EventKind {
    /// Subscription: a tracked publisher's liveliness state changed
    /// (started / stopped asserting).
    LivelinessChanged = 0,
    /// Subscription: an expected sample didn't arrive within the
    /// configured deadline.
    RequestedDeadlineMissed = 1,
    /// Subscription: the backend dropped a sample (overflow / etc.).
    MessageLost = 2,
    /// Publisher: this publisher missed its own liveliness assertion.
    LivelinessLost = 3,
    /// Publisher: this publisher promised X Hz, fell behind.
    OfferedDeadlineMissed = 4,
    /// Publisher: the set of subscriptions matched to it changed. Upstream
    /// `RMW_EVENT_PUBLICATION_MATCHED` (Iron+), issue 1972.
    PublicationMatched = 5,
    /// Subscription: the set of publishers matched to it changed. Upstream
    /// `RMW_EVENT_SUBSCRIPTION_MATCHED`.
    SubscriptionMatched = 6,
    /// Publisher: an endpoint on the same topic was seen with a different
    /// type. Upstream `RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE`.
    PublisherIncompatibleType = 7,
    /// Subscription: an endpoint on the same topic was seen with a different
    /// type. Upstream `RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE`.
    SubscriptionIncompatibleType = 8,
}

impl EventKind {
    /// `true` for a kind a PUBLISHER carries, `false` for a subscription's.
    pub const fn is_publisher_side(self) -> bool {
        matches!(
            self,
            EventKind::LivelinessLost
                | EventKind::OfferedDeadlineMissed
                | EventKind::PublicationMatched
                | EventKind::PublisherIncompatibleType
        )
    }
}

/// Liveliness-status payload. Mirrors DDS
/// `rmw_liveliness_changed_status_t` shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct LivelinessChangedStatus {
    /// Number of currently-alive matched publishers (subscriber side)
    /// or self-asserting state (publisher side, always 0/1).
    pub alive_count: u16,
    /// Number of currently-not-alive matched publishers.
    pub not_alive_count: u16,
    /// Change in `alive_count` since the last callback fire.
    pub alive_count_change: i16,
    /// Change in `not_alive_count` since the last callback fire.
    pub not_alive_count_change: i16,
}

/// Deadline / message-lost payload. Used for
/// `RequestedDeadlineMissed`, `LivelinessLost`,
/// `OfferedDeadlineMissed`, `MessageLost` — same shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct CountStatus {
    /// Cumulative count over the entity's lifetime.
    pub total_count: u32,
    /// Change since the last callback fire.
    pub total_count_change: u32,
}

/// Matched payload — upstream `rmw_matched_status_t` (Iron+), issue 1972.
///
/// Same four fields in upstream's order. The counts are `u32` where upstream
/// has `size_t`, like every other count on this surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct MatchedStatus {
    /// Cumulative matches over the entity's lifetime.
    pub total_count: u32,
    /// Change in `total_count` since the last report.
    pub total_count_change: u32,
    /// Endpoints matched right now.
    pub current_count: u32,
    /// Change in `current_count` since the last report (negative when an
    /// endpoint left).
    pub current_count_change: i32,
}

/// Incompatible-type payload — upstream `rmw_incompatible_type_status_t`
/// (Iron+), issue 1972. `u32` where upstream has `int32_t`: both only grow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct IncompatibleTypeStatus {
    /// Cumulative incompatible endpoints seen over the entity's lifetime.
    pub total_count: u32,
    /// Change since the last report.
    pub total_count_change: u32,
}

/// Turns successive SNAPSHOTS of a matched count into [`MatchedStatus`]
/// reports — issue 1972. `no_std`, no allocation, eight bytes.
///
/// A backend that can count its matched peers but has no change counter of
/// its own (zenoh's graph cache; a DDS backend whose change counters are
/// reset by other readers of the same status) feeds the current count here
/// on every poll, and gets a report only when it moved.
///
/// **`total_count` counts INCREASES SEEN between polls.** A peer that leaves
/// and another that joins between two polls nets to no change and is not
/// counted; upstream's DDS counter would count the join. A snapshot cannot
/// do better, and saying so is the honest bound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchedCounter {
    total: u32,
    current: u32,
}

impl MatchedCounter {
    /// A counter that has seen nothing matched yet.
    pub const fn new() -> Self {
        Self {
            total: 0,
            current: 0,
        }
    }

    /// The last `current_count` observed.
    pub const fn current(&self) -> u32 {
        self.current
    }

    /// Record a snapshot of how many endpoints are matched now. `Some` with
    /// the report when the count moved, `None` when it did not.
    pub fn observe(&mut self, current_now: u32) -> Option<MatchedStatus> {
        if current_now == self.current {
            return None;
        }
        let grew = current_now.saturating_sub(self.current);
        let change = (current_now as i64 - self.current as i64)
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        self.total = self.total.saturating_add(grew);
        self.current = current_now;
        Some(MatchedStatus {
            total_count: self.total,
            total_count_change: grew,
            current_count: current_now,
            current_count_change: change,
        })
    }
}

/// Turns successive snapshots of an incompatible-type count into
/// [`IncompatibleTypeStatus`] reports. Same bound as [`MatchedCounter`]:
/// only increases between polls are counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IncompatibleTypeCounter {
    total: u32,
    current: u32,
}

impl IncompatibleTypeCounter {
    /// A counter that has seen nothing yet.
    pub const fn new() -> Self {
        Self {
            total: 0,
            current: 0,
        }
    }

    /// Record how many incompatible endpoints are visible now. `Some` when
    /// the number GREW (a new incompatible endpoint), `None` otherwise — a
    /// mismatched endpoint leaving is not an event upstream reports either.
    pub fn observe(&mut self, current_now: u32) -> Option<IncompatibleTypeStatus> {
        let grew = current_now.saturating_sub(self.current);
        self.current = current_now;
        if grew == 0 {
            return None;
        }
        self.total = self.total.saturating_add(grew);
        Some(IncompatibleTypeStatus {
            total_count: self.total,
            total_count_change: grew,
        })
    }
}

/// Type alias: the deadline-missed shape is identical to
/// [`CountStatus`].
pub type DeadlineMissedStatus = CountStatus;

/// Type alias: the message-lost shape is identical to [`CountStatus`].
pub type MessageLostStatus = CountStatus;

/// Borrow-shaped event payload union. Variant selection mirrors
/// [`EventKind`].
#[derive(Debug, Clone, Copy)]
pub enum EventPayload<'a> {
    LivelinessChanged(&'a LivelinessChangedStatus),
    RequestedDeadlineMissed(&'a DeadlineMissedStatus),
    MessageLost(&'a MessageLostStatus),
    LivelinessLost(&'a CountStatus),
    OfferedDeadlineMissed(&'a DeadlineMissedStatus),
    PublicationMatched(&'a MatchedStatus),
    SubscriptionMatched(&'a MatchedStatus),
    PublisherIncompatibleType(&'a IncompatibleTypeStatus),
    SubscriptionIncompatibleType(&'a IncompatibleTypeStatus),
}

impl<'a> EventPayload<'a> {
    /// Returns the [`EventKind`] this payload corresponds to.
    pub fn kind(&self) -> EventKind {
        match self {
            EventPayload::LivelinessChanged(_) => EventKind::LivelinessChanged,
            EventPayload::RequestedDeadlineMissed(_) => EventKind::RequestedDeadlineMissed,
            EventPayload::MessageLost(_) => EventKind::MessageLost,
            EventPayload::LivelinessLost(_) => EventKind::LivelinessLost,
            EventPayload::OfferedDeadlineMissed(_) => EventKind::OfferedDeadlineMissed,
            EventPayload::PublicationMatched(_) => EventKind::PublicationMatched,
            EventPayload::SubscriptionMatched(_) => EventKind::SubscriptionMatched,
            EventPayload::PublisherIncompatibleType(_) => EventKind::PublisherIncompatibleType,
            EventPayload::SubscriptionIncompatibleType(_) => {
                EventKind::SubscriptionIncompatibleType
            }
        }
    }
}

/// Raw event callback. Identical Rust + C ABI; no_std + alloc-free.
///
/// The backend invokes this with the [`EventKind`] selecting which
/// payload-pointer variant is valid. `user_ctx` is opaque application
/// state passed at registration.
///
/// **Lifetime.** `payload_ptr` is valid for the duration of this call
/// only. Copy fields out before returning if needed beyond.
///
/// **Threading.** Invoked from inside `drive_io` on the executor
/// thread; do not block.
///
/// **Safety contract.** The callback is `unsafe extern "C" fn`:
/// implementors guarantee `payload_ptr` is non-null and points to a
/// valid payload of the variant selected by `kind`.
pub type EventCallback = unsafe extern "C" fn(
    kind: EventKind,
    payload_ptr: *const core::ffi::c_void,
    user_ctx: *mut core::ffi::c_void,
);

/// Helper: convert a raw `(kind, payload_ptr)` pair into a typed
/// [`EventPayload`] borrow. Used inside `EventCallback` trampolines.
///
/// # Safety
///
/// Caller guarantees `payload_ptr` matches the variant indicated by
/// `kind` and is valid for the borrow lifetime.
pub unsafe fn payload_from_raw<'a>(
    kind: EventKind,
    payload_ptr: *const core::ffi::c_void,
) -> EventPayload<'a> {
    match kind {
        EventKind::LivelinessChanged => unsafe {
            EventPayload::LivelinessChanged(&*(payload_ptr as *const LivelinessChangedStatus))
        },
        EventKind::RequestedDeadlineMissed => unsafe {
            EventPayload::RequestedDeadlineMissed(&*(payload_ptr as *const CountStatus))
        },
        EventKind::MessageLost => unsafe {
            EventPayload::MessageLost(&*(payload_ptr as *const CountStatus))
        },
        EventKind::LivelinessLost => unsafe {
            EventPayload::LivelinessLost(&*(payload_ptr as *const CountStatus))
        },
        EventKind::OfferedDeadlineMissed => unsafe {
            EventPayload::OfferedDeadlineMissed(&*(payload_ptr as *const CountStatus))
        },
        EventKind::PublicationMatched => unsafe {
            EventPayload::PublicationMatched(&*(payload_ptr as *const MatchedStatus))
        },
        EventKind::SubscriptionMatched => unsafe {
            EventPayload::SubscriptionMatched(&*(payload_ptr as *const MatchedStatus))
        },
        EventKind::PublisherIncompatibleType => unsafe {
            EventPayload::PublisherIncompatibleType(
                &*(payload_ptr as *const IncompatibleTypeStatus),
            )
        },
        EventKind::SubscriptionIncompatibleType => unsafe {
            EventPayload::SubscriptionIncompatibleType(
                &*(payload_ptr as *const IncompatibleTypeStatus),
            )
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_counter_reports_only_movement_and_signs_the_change() {
        let mut c = MatchedCounter::new();
        assert_eq!(c.observe(0), None, "0 -> 0 is no event");
        let up = c.observe(1).expect("0 -> 1");
        assert_eq!(
            up,
            MatchedStatus {
                total_count: 1,
                total_count_change: 1,
                current_count: 1,
                current_count_change: 1,
            }
        );
        assert_eq!(c.observe(1), None, "1 -> 1 is no event");
        let up2 = c.observe(3).expect("1 -> 3");
        assert_eq!((up2.total_count, up2.total_count_change), (3, 2));
        let down = c.observe(0).expect("3 -> 0");
        assert_eq!(
            down,
            MatchedStatus {
                total_count: 3,
                total_count_change: 0,
                current_count: 0,
                current_count_change: -3,
            }
        );
        assert_eq!(c.current(), 0);
    }

    #[test]
    fn incompatible_type_counter_reports_growth_only() {
        let mut c = IncompatibleTypeCounter::new();
        assert_eq!(c.observe(0), None);
        assert_eq!(
            c.observe(2),
            Some(IncompatibleTypeStatus {
                total_count: 2,
                total_count_change: 2
            })
        );
        assert_eq!(
            c.observe(1),
            None,
            "a mismatched endpoint leaving is not an event"
        );
        assert_eq!(
            c.observe(2),
            Some(IncompatibleTypeStatus {
                total_count: 3,
                total_count_change: 1
            })
        );
    }

    #[test]
    fn side_split_covers_every_kind() {
        use EventKind::*;
        for k in [
            LivelinessLost,
            OfferedDeadlineMissed,
            PublicationMatched,
            PublisherIncompatibleType,
        ] {
            assert!(k.is_publisher_side(), "{k:?}");
        }
        for k in [
            LivelinessChanged,
            RequestedDeadlineMissed,
            MessageLost,
            SubscriptionMatched,
            SubscriptionIncompatibleType,
        ] {
            assert!(!k.is_publisher_side(), "{k:?}");
        }
    }

    #[test]
    fn payload_round_trips_through_the_raw_pointer() {
        let m = MatchedStatus {
            total_count: 4,
            total_count_change: 1,
            current_count: 2,
            current_count_change: -1,
        };
        let p = unsafe {
            payload_from_raw(
                EventKind::SubscriptionMatched,
                &m as *const _ as *const core::ffi::c_void,
            )
        };
        assert_eq!(p.kind(), EventKind::SubscriptionMatched);
        match p {
            EventPayload::SubscriptionMatched(s) => assert_eq!(*s, m),
            other => panic!("wrong arm: {other:?}"),
        }
    }
}
