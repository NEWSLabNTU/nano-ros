//! Status-event surface (Phase 108).
//!
//! Tier-1 events that backends optionally surface — liveliness
//! changes, deadline misses, message loss. Dispatched via
//! callback-on-entity (registration on `Subscriber` / `Publisher`),
//! not via upstream's waitset-take pattern.
//!
//! Events fire from inside [`Session::drive_io`](crate::Session::drive_io)
//! on the executor thread, the same as message callbacks. They count
//! against the executor's `max_callbacks_per_spin` cap (Phase 105) the
//! same way.
//!
//! See `book/src/concepts/status-events.md` for the user-facing
//! patterns and `book/src/design/rmw-vs-upstream.md` Section 8 for
//! the design rationale.
//!
//! ### Tier-2 / Tier-3 deliberately skipped
//!
//! Upstream `rmw_event_type_t` includes `MATCHED`, `QOS_INCOMPATIBLE`,
//! and `INCOMPATIBLE_TYPE`. The first is deferred until dynamic-
//! discovery use cases appear; additive without an ABI break. The
//! latter two are surfaced synchronously at create time as
//! `TransportError::IncompatibleQos` / `TopicNameInvalid` (mapped to
//! `NROS_RMW_RET_INCOMPATIBLE_QOS` / `NROS_RMW_RET_TOPIC_NAME_INVALID`
//! at the C boundary). No runtime event needed.

#[cfg(feature = "alloc")]
extern crate alloc;

/// Tier-1 status-event kinds. `#[non_exhaustive]` so adding a Tier-2
/// (`MATCHED`) variant later is not an ABI break for matchers.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EventKind {
    /// Subscriber: a tracked publisher's liveliness state changed
    /// (started / stopped asserting).
    LivelinessChanged = 0,
    /// Subscriber: an expected sample didn't arrive within the
    /// configured deadline.
    RequestedDeadlineMissed = 1,
    /// Subscriber: the backend dropped a sample (overflow / etc.).
    MessageLost = 2,
    /// Publisher: this publisher missed its own liveliness assertion.
    LivelinessLost = 3,
    /// Publisher: this publisher promised X Hz, fell behind.
    OfferedDeadlineMissed = 4,
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
        }
    }
}

/// Heap-allocated event-callback closure. Available with the `alloc`
/// feature. Backends may also accept `'static fn(&EventPayload)`
/// pointers via a separate registration path on no-alloc targets;
/// not yet exposed.
#[cfg(feature = "alloc")]
pub type EventCallback = alloc::boxed::Box<dyn FnMut(EventPayload<'_>) + Send>;
