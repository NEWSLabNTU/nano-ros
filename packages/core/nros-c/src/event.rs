//! Phase 108 — status-event API for the nros C user-facing surface.
//!
//! User-facing typedefs + setter functions for Tier-1 status events
//! (liveliness, deadline, message-lost). The functions return
//! `NROS_RMW_RET_UNSUPPORTED` until backend wiring lands per-phase
//! (109+); applications can compile against the API today and the
//! events start firing as backends opt in.

use core::ffi::c_void;

use crate::error::*;
use crate::publisher::nros_publisher_t;
use crate::subscription::nros_subscription_t;

/// Tier-1 status-event kinds. Stable u8 values matching
/// `nros_rmw_event_kind_t` in `<nros/rmw_event.h>`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_event_kind_t {
    /// Subscriber: a tracked publisher's liveliness state changed.
    NROS_EVENT_LIVELINESS_CHANGED = 0,
    /// Subscriber: an expected sample didn't arrive in time.
    NROS_EVENT_REQUESTED_DEADLINE_MISSED = 1,
    /// Subscriber: backend dropped a sample.
    NROS_EVENT_MESSAGE_LOST = 2,
    /// Publisher: this publisher missed its own liveliness assertion.
    NROS_EVENT_LIVELINESS_LOST = 3,
    /// Publisher: this publisher fell behind its offered rate.
    NROS_EVENT_OFFERED_DEADLINE_MISSED = 4,
}

/// Liveliness payload. Matches DDS `rmw_liveliness_changed_status_t`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct nros_liveliness_changed_status_t {
    pub alive_count: u16,
    pub not_alive_count: u16,
    pub alive_count_change: i16,
    pub not_alive_count_change: i16,
}

/// Count payload. Used for deadline-missed and message-lost events.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct nros_count_status_t {
    pub total_count: u32,
    pub total_count_change: u32,
}

// ============================================================================
// Subscription event callbacks
// ============================================================================

/// Callback for `NROS_EVENT_LIVELINESS_CHANGED`.
pub type nros_event_liveliness_changed_cb_t = Option<
    unsafe extern "C" fn(
        sub: *mut nros_subscription_t,
        status: nros_liveliness_changed_status_t,
        user_context: *mut c_void,
    ),
>;

/// Callback for `NROS_EVENT_REQUESTED_DEADLINE_MISSED`,
/// `NROS_EVENT_MESSAGE_LOST` (subscriber side).
pub type nros_event_subscriber_count_cb_t = Option<
    unsafe extern "C" fn(
        sub: *mut nros_subscription_t,
        status: nros_count_status_t,
        user_context: *mut c_void,
    ),
>;

/// Register a callback for `NROS_EVENT_LIVELINESS_CHANGED`.
///
/// Returns `NROS_RMW_RET_UNSUPPORTED` until the active backend wires
/// up liveliness-event detection (Phase 109+). Applications can call
/// it today; the event fires once the backend gains support.
///
/// # Safety
///
/// `sub` must point to a valid, initialised `nros_subscription_t`.
/// `cb` must be a valid function pointer or `None`. `user_context`
/// is opaque to nros and must outlive the subscription.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_set_liveliness_changed_callback(
    _sub: *mut nros_subscription_t,
    _cb: nros_event_liveliness_changed_cb_t,
    _user_context: *mut c_void,
) -> nros_ret_t {
    // Backend wiring lands per-phase (109+); for now, the API surface
    // is exposed but no backend implements the underlying event.
    NROS_RET_UNSUPPORTED
}

/// Register a callback for `NROS_EVENT_REQUESTED_DEADLINE_MISSED`.
///
/// `deadline_ms` is the maximum acceptable inter-arrival time for
/// expected samples. When a sample doesn't arrive within this window,
/// the callback fires.
///
/// # Safety
///
/// See [`nros_subscription_set_liveliness_changed_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_set_requested_deadline_missed_callback(
    _sub: *mut nros_subscription_t,
    _deadline_ms: u32,
    _cb: nros_event_subscriber_count_cb_t,
    _user_context: *mut c_void,
) -> nros_ret_t {
    NROS_RET_UNSUPPORTED
}

/// Register a callback for `NROS_EVENT_MESSAGE_LOST`.
///
/// # Safety
///
/// See [`nros_subscription_set_liveliness_changed_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_subscription_set_message_lost_callback(
    _sub: *mut nros_subscription_t,
    _cb: nros_event_subscriber_count_cb_t,
    _user_context: *mut c_void,
) -> nros_ret_t {
    NROS_RET_UNSUPPORTED
}

// ============================================================================
// Publisher event callbacks
// ============================================================================

/// Callback for `NROS_EVENT_LIVELINESS_LOST`,
/// `NROS_EVENT_OFFERED_DEADLINE_MISSED` (publisher side).
pub type nros_event_publisher_count_cb_t = Option<
    unsafe extern "C" fn(
        pub_: *mut nros_publisher_t,
        status: nros_count_status_t,
        user_context: *mut c_void,
    ),
>;

/// Register a callback for `NROS_EVENT_LIVELINESS_LOST`.
///
/// # Safety
///
/// `pub_` must point to a valid, initialised `nros_publisher_t`.
/// `cb` must be a valid function pointer or `None`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_set_liveliness_lost_callback(
    _pub_: *mut nros_publisher_t,
    _cb: nros_event_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> nros_ret_t {
    NROS_RET_UNSUPPORTED
}

/// Register a callback for `NROS_EVENT_OFFERED_DEADLINE_MISSED`.
///
/// `deadline_ms` is the publisher's offered minimum-rate window.
///
/// # Safety
///
/// See [`nros_publisher_set_liveliness_lost_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_publisher_set_offered_deadline_missed_callback(
    _pub_: *mut nros_publisher_t,
    _deadline_ms: u32,
    _cb: nros_event_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> nros_ret_t {
    NROS_RET_UNSUPPORTED
}
