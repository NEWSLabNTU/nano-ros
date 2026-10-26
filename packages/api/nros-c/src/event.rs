//! Phase 108 — status-event API for the nros C user-facing surface.
//!
//! User-facing typedefs + setter functions for status events. All nine
//! setters — the five Tier-1 kinds (liveliness, deadline, message-lost,
//! issue 1993) and the four issue-1972 kinds (matched, incompatible type) —
//! reach the backend through [`status_event_slots`]. Whether an event is
//! carried is the backend's answer (`supports_event`, which the cffi route
//! takes from `backend_event_type_is_supported`): Cyclone DDS and zenoh carry
//! all nine, XRCE-DDS and uORB none, and those answer `RCL_RET_UNSUPPORTED`.

use core::ffi::c_void;

use crate::{error::*, publisher::rcl_publisher_t, subscription::rcl_subscription_t};
use nros_node::executor::ffi::ExecutorFfi as _;

/// Tier-1 status-event kinds. Stable u8 values matching
/// `rmw_event_type_t` in `<nros/rmw_event.h>`.
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
    /// Publisher: a subscription started or stopped matching (issue 1972).
    NROS_EVENT_PUBLICATION_MATCHED = 5,
    /// Subscriber: a publisher started or stopped matching (issue 1972).
    NROS_EVENT_SUBSCRIPTION_MATCHED = 6,
    /// Publisher: a subscription on this topic name has another type.
    NROS_EVENT_PUBLISHER_INCOMPATIBLE_TYPE = 7,
    /// Subscriber: a publisher on this topic name has another type.
    NROS_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE = 8,
}

/// Liveliness payload. Matches DDS `rmw_liveliness_changed_status_t`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct rclc_liveliness_changed_status_t {
    pub alive_count: u16,
    pub not_alive_count: u16,
    pub alive_count_change: i16,
    pub not_alive_count_change: i16,
}

/// Count payload. Used for deadline-missed and message-lost events.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct rclc_count_status_t {
    pub total_count: u32,
    pub total_count_change: u32,
}

// ============================================================================
// Subscription event callbacks
// ============================================================================

/// Callback for `NROS_EVENT_LIVELINESS_CHANGED`.
pub type rclc_event_liveliness_changed_cb_t = Option<
    unsafe extern "C" fn(
        sub: *mut rcl_subscription_t,
        status: rclc_liveliness_changed_status_t,
        user_context: *mut c_void,
    ),
>;

/// Callback for `NROS_EVENT_REQUESTED_DEADLINE_MISSED`,
/// `NROS_EVENT_MESSAGE_LOST` (subscriber side).
pub type rclc_event_subscriber_count_cb_t = Option<
    unsafe extern "C" fn(
        sub: *mut rcl_subscription_t,
        status: rclc_count_status_t,
        user_context: *mut c_void,
    ),
>;

/// Register a callback for `NROS_EVENT_LIVELINESS_CHANGED` — upstream
/// `RMW_EVENT_LIVELINESS_CHANGED`. Fires when a matched publisher starts or
/// stops asserting its liveliness; observed as the executor polls the
/// subscription.
///
/// The subscription must already be ADDED to an executor
/// (`rclc_executor_add_subscription*`): the executor owns the subscriber, so
/// before that there is nothing to register on. Registering again for the
/// same subscription replaces the earlier callback.
///
/// # Returns
/// * `RCL_RET_OK` — registered
/// * `RCL_RET_INVALID_ARGUMENT` — NULL `sub` or `cb`
/// * `RCL_RET_NOT_INIT` — the subscription is not initialised, or not yet
///   added to an executor
/// * `RCL_RET_UNSUPPORTED` — the backend does not generate this event
///   (XRCE-DDS, uORB; zenoh on bare metal). Decided by the backend, never
///   by this layer.
/// * `RCLC_RET_FULL` — the process-wide event-slot table is full: it holds
///   16 registrations across every C and C++ entity, and one publisher
///   with all four of its kinds plus one subscription with all five fit
///
/// # Safety
///
/// `sub` must point to a valid, initialised `rcl_subscription_t`.
/// `cb` must be a valid function pointer or `None`. `user_context`
/// is opaque to nros and must outlive the subscription.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_liveliness_changed_callback(
    sub: *mut rcl_subscription_t,
    cb: rclc_event_liveliness_changed_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            subscription_set(
                sub,
                nros_rmw::EventKind::LivelinessChanged,
                0,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (sub, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

/// Register a callback for `NROS_EVENT_REQUESTED_DEADLINE_MISSED` —
/// upstream `RMW_EVENT_REQUESTED_DEADLINE_MISSED`.
///
/// `deadline_ms` is the maximum acceptable inter-arrival time for expected
/// samples; it is handed to the backend's registration. zenoh uses it when
/// the subscription's QoS states no deadline of its own; Cyclone DDS takes
/// the deadline from the QoS profile and ignores it.
///
/// Same contract and returns as
/// [`rclc_subscription_set_liveliness_changed_callback`].
///
/// # Safety
///
/// See [`rclc_subscription_set_liveliness_changed_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_requested_deadline_missed_callback(
    sub: *mut rcl_subscription_t,
    deadline_ms: u32,
    cb: rclc_event_subscriber_count_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            subscription_set(
                sub,
                nros_rmw::EventKind::RequestedDeadlineMissed,
                deadline_ms,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (sub, deadline_ms, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

/// Register a callback for `NROS_EVENT_MESSAGE_LOST` — upstream
/// `RMW_EVENT_MESSAGE_LOST`. Fires when the backend reports a dropped sample.
///
/// Same contract and returns as
/// [`rclc_subscription_set_liveliness_changed_callback`].
///
/// # Safety
///
/// See [`rclc_subscription_set_liveliness_changed_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_message_lost_callback(
    sub: *mut rcl_subscription_t,
    cb: rclc_event_subscriber_count_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe { subscription_set(sub, nros_rmw::EventKind::MessageLost, 0, cb, user_context) }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (sub, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

// ============================================================================
// Publisher event callbacks
// ============================================================================

/// Callback for `NROS_EVENT_LIVELINESS_LOST`,
/// `NROS_EVENT_OFFERED_DEADLINE_MISSED` (publisher side).
pub type rclc_event_publisher_count_cb_t = Option<
    unsafe extern "C" fn(
        pub_: *mut rcl_publisher_t,
        status: rclc_count_status_t,
        user_context: *mut c_void,
    ),
>;

/// Register a callback for `NROS_EVENT_LIVELINESS_LOST` — upstream
/// `RMW_EVENT_LIVELINESS_LOST`. Fires when this publisher misses its own
/// liveliness assertion; observed on the publisher's poll points
/// (`rcl_publish`, `rcl_publisher_assert_liveliness`).
///
/// Registering again for the same publisher replaces the earlier callback.
///
/// # Returns
/// * `RCL_RET_OK` — registered
/// * `RCL_RET_INVALID_ARGUMENT` — NULL `pub_` or `cb`
/// * `RCL_RET_NOT_INIT` — the publisher is not initialised
/// * `RCL_RET_UNSUPPORTED` — the backend does not generate this event
///   (XRCE-DDS, uORB). Decided by the backend, never by this layer.
/// * `RCLC_RET_FULL` — the process-wide event-slot table is full: it holds
///   16 registrations across every C and C++ entity, and one publisher
///   with all four of its kinds plus one subscription with all five fit
///
/// # Safety
///
/// `pub_` must point to a valid, initialised `rcl_publisher_t` that is not
/// moved while registered. `cb` must be a valid function pointer or `None`;
/// `user_context` must outlive the publisher.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_set_liveliness_lost_callback(
    pub_: *mut rcl_publisher_t,
    cb: rclc_event_publisher_count_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            publisher_set(
                pub_,
                nros_rmw::EventKind::LivelinessLost,
                0,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (pub_, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

/// Register a callback for `NROS_EVENT_OFFERED_DEADLINE_MISSED` — upstream
/// `RMW_EVENT_OFFERED_DEADLINE_MISSED`.
///
/// `deadline_ms` is the publisher's offered minimum-rate window; it is handed
/// to the backend's registration. zenoh uses it when the publisher's QoS
/// states no deadline of its own; Cyclone DDS takes the deadline from the QoS
/// profile and ignores it.
///
/// Same contract and returns as
/// [`rclc_publisher_set_liveliness_lost_callback`].
///
/// # Safety
///
/// See [`rclc_publisher_set_liveliness_lost_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_set_offered_deadline_missed_callback(
    pub_: *mut rcl_publisher_t,
    deadline_ms: u32,
    cb: rclc_event_publisher_count_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            publisher_set(
                pub_,
                nros_rmw::EventKind::OfferedDeadlineMissed,
                deadline_ms,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (pub_, deadline_ms, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

// ============================================================================
// Issue 1972 — matched and incompatible-type events
// ============================================================================
//
// Every setter in this file needs a place to keep the user's `(callback,
// context, entity)` triple: the backend's status-event channel carries ONE
// opaque pointer, and the C entity's layout is a generated header users
// allocate, so the triple lives in [`status_event_slots`] — a small static
// table shared with the C++ API — and the backend is handed a tagged slot
// reference instead.

/// Matched payload — upstream `rmw_matched_status_t` (Iron+), rclcpp's
/// `MatchedInfo`. Same four fields in upstream's order; the counts are
/// `uint32_t` where upstream has `size_t`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct rclc_matched_status_t {
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
/// (Iron+), rclcpp's `IncompatibleTypeInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct rclc_incompatible_type_status_t {
    /// Cumulative incompatible endpoints seen over the entity's lifetime.
    pub total_count: u32,
    /// Change since the last report.
    pub total_count_change: u32,
}

/// Callback for `NROS_EVENT_PUBLICATION_MATCHED`.
pub type rclc_event_publisher_matched_cb_t = Option<
    unsafe extern "C" fn(
        pub_: *mut rcl_publisher_t,
        status: rclc_matched_status_t,
        user_context: *mut c_void,
    ),
>;

/// Callback for `NROS_EVENT_PUBLISHER_INCOMPATIBLE_TYPE`.
pub type rclc_event_publisher_incompatible_type_cb_t = Option<
    unsafe extern "C" fn(
        pub_: *mut rcl_publisher_t,
        status: rclc_incompatible_type_status_t,
        user_context: *mut c_void,
    ),
>;

/// Callback for `NROS_EVENT_SUBSCRIPTION_MATCHED`.
pub type rclc_event_subscription_matched_cb_t = Option<
    unsafe extern "C" fn(
        sub: *mut rcl_subscription_t,
        status: rclc_matched_status_t,
        user_context: *mut c_void,
    ),
>;

/// Callback for `NROS_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE`.
pub type rclc_event_subscription_incompatible_type_cb_t = Option<
    unsafe extern "C" fn(
        sub: *mut rcl_subscription_t,
        status: rclc_incompatible_type_status_t,
        user_context: *mut c_void,
    ),
>;

#[cfg(feature = "rmw-cffi")]
const _: () = {
    use core::mem::{align_of, size_of};
    // The slot trampoline calls the user's callback with the `nros_rmw`
    // payload BY VALUE; that is sound only while the two layouts agree.
    assert!(size_of::<rclc_matched_status_t>() == size_of::<nros_rmw::MatchedStatus>());
    assert!(align_of::<rclc_matched_status_t>() == align_of::<nros_rmw::MatchedStatus>());
    assert!(
        size_of::<rclc_incompatible_type_status_t>()
            == size_of::<nros_rmw::IncompatibleTypeStatus>()
    );
    assert!(
        align_of::<rclc_incompatible_type_status_t>()
            == align_of::<nros_rmw::IncompatibleTypeStatus>()
    );
};

#[cfg(feature = "rmw-cffi")]
const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    use nros_rmw::{CountStatus as R, LivelinessChangedStatus as L};
    // Issue 1993 — the Tier-1 payloads, by value, for the same reason.
    assert!(size_of::<rclc_liveliness_changed_status_t>() == size_of::<L>());
    assert!(align_of::<rclc_liveliness_changed_status_t>() == align_of::<L>());
    assert!(
        offset_of!(rclc_liveliness_changed_status_t, alive_count) == offset_of!(L, alive_count)
    );
    assert!(
        offset_of!(rclc_liveliness_changed_status_t, not_alive_count)
            == offset_of!(L, not_alive_count)
    );
    assert!(
        offset_of!(rclc_liveliness_changed_status_t, alive_count_change)
            == offset_of!(L, alive_count_change)
    );
    assert!(
        offset_of!(rclc_liveliness_changed_status_t, not_alive_count_change)
            == offset_of!(L, not_alive_count_change)
    );
    assert!(size_of::<rclc_count_status_t>() == size_of::<R>());
    assert!(align_of::<rclc_count_status_t>() == align_of::<R>());
    assert!(offset_of!(rclc_count_status_t, total_count) == offset_of!(R, total_count));
    assert!(
        offset_of!(rclc_count_status_t, total_count_change) == offset_of!(R, total_count_change)
    );
};

/// Map a registration verdict onto `rcl_ret_t`.
#[cfg(feature = "rmw-cffi")]
fn slot_verdict(r: Result<(), nros_rmw::TransportError>) -> rcl_ret_t {
    match r {
        Ok(()) => RCL_RET_OK,
        Err(nros_rmw::TransportError::BadAlloc) => RCLC_RET_FULL,
        Err(nros_rmw::TransportError::InvalidArgument) => RCL_RET_INVALID_ARGUMENT,
        Err(_) => RCL_RET_UNSUPPORTED,
    }
}

/// Register on an inline-storage publisher. `deadline_ms` is forwarded to the
/// backend's registration (`0` for the kinds that take none, as `nros-node`
/// does).
#[cfg(feature = "rmw-cffi")]
unsafe fn publisher_set(
    pub_: *mut rcl_publisher_t,
    kind: nros_rmw::EventKind,
    deadline_ms: u32,
    cb: *const (),
    user_context: *mut c_void,
) -> rcl_ret_t {
    if pub_.is_null() || cb.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let publisher = unsafe { &mut *pub_ };
    if publisher.state != crate::publisher::rclc_publisher_state_t::RCLC_PUBLISHER_STATE_INITIALIZED
    {
        return RCL_RET_NOT_INIT;
    }
    let handle = publisher._opaque.as_mut_ptr() as *mut nros::internals::RmwPublisher;
    // Issue 1993 — the backend decides whether it carries `kind`; this layer
    // never answers for it.
    if !nros_rmw::Publisher::supports_event(unsafe { &*handle }, kind) {
        return RCL_RET_UNSUPPORTED;
    }
    let r = unsafe {
        status_event_slots::register(
            handle as usize,
            pub_ as *mut c_void,
            kind,
            cb,
            user_context,
            |k, tramp, tag| {
                nros_rmw::Publisher::register_event_callback(
                    &mut *handle,
                    k,
                    deadline_ms,
                    tramp,
                    tag,
                )
            },
        )
    };
    slot_verdict(r)
}

/// Register on an executor-owned subscription. `deadline_ms` as for
/// [`publisher_set`].
#[cfg(feature = "rmw-cffi")]
unsafe fn subscription_set(
    sub: *mut rcl_subscription_t,
    kind: nros_rmw::EventKind,
    deadline_ms: u32,
    cb: *const (),
    user_context: *mut c_void,
) -> rcl_ret_t {
    if sub.is_null() || cb.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let subscription = unsafe { &mut *sub };
    if subscription.state
        != crate::subscription::rclc_subscription_state_t::RCLC_SUBSCRIPTION_STATE_INITIALIZED
        || subscription._executor.is_null()
        || subscription.handle_id == usize::MAX
    {
        // Not yet added to an executor: there is no subscriber to ask.
        return RCL_RET_NOT_INIT;
    }
    let exec_t = subscription._executor as *mut crate::executor::rclc_executor_t;
    let mut exec = unsafe { crate::executor::get_executor(exec_t) };
    let Some(handle) = (unsafe { exec.subscription_handle_mut(subscription.handle_id) }) else {
        return RCL_RET_NOT_INIT;
    };
    let handle = handle as *mut nros::internals::RmwSubscriber;
    if !nros_rmw::Subscription::supports_event(unsafe { &*handle }, kind) {
        return RCL_RET_UNSUPPORTED;
    }
    let r = unsafe {
        status_event_slots::register(
            handle as usize,
            sub as *mut c_void,
            kind,
            cb,
            user_context,
            |k, tramp, tag| {
                nros_rmw::Subscription::register_event_callback(
                    &mut *handle,
                    k,
                    deadline_ms,
                    tramp,
                    tag,
                )
            },
        )
    };
    slot_verdict(r)
}

/// Register a callback for `NROS_EVENT_PUBLICATION_MATCHED` — issue 1972,
/// upstream `RMW_EVENT_PUBLICATION_MATCHED`. Fires when a subscription starts
/// or stops matching this publisher.
///
/// Observed on the publisher's poll points (`rcl_publish`,
/// `rcl_publisher_assert_liveliness`) on both backends that carry it
/// (Cyclone DDS, zenoh). Registering again for the same publisher replaces
/// the earlier callback.
///
/// # Returns
/// * `RCL_RET_OK` — registered
/// * `RCL_RET_INVALID_ARGUMENT` — NULL `pub_` or `cb`
/// * `RCL_RET_NOT_INIT` — the publisher is not initialised
/// * `RCL_RET_UNSUPPORTED` — the backend does not generate this event
/// * `RCLC_RET_FULL` — the process-wide event-slot table
///   (16 registrations, process-wide) is full
///
/// # Safety
/// `pub_` must point to a valid `rcl_publisher_t` that is not moved while
/// registered; `user_context` must outlive it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_set_matched_callback(
    pub_: *mut rcl_publisher_t,
    cb: rclc_event_publisher_matched_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            publisher_set(
                pub_,
                nros_rmw::EventKind::PublicationMatched,
                0,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (pub_, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

/// Register a callback for `NROS_EVENT_PUBLISHER_INCOMPATIBLE_TYPE` — issue
/// 1972. Fires when a subscription on this topic NAME with a different type
/// is seen. Same returns and safety as
/// [`rclc_publisher_set_matched_callback`].
///
/// # Safety
/// See [`rclc_publisher_set_matched_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_set_incompatible_type_callback(
    pub_: *mut rcl_publisher_t,
    cb: rclc_event_publisher_incompatible_type_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            publisher_set(
                pub_,
                nros_rmw::EventKind::PublisherIncompatibleType,
                0,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (pub_, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

/// Register a callback for `NROS_EVENT_SUBSCRIPTION_MATCHED` — issue 1972,
/// upstream `RMW_EVENT_SUBSCRIPTION_MATCHED`. Fires when a publisher starts
/// or stops matching this subscription; observed as the executor polls it.
///
/// The subscription must already be ADDED to an executor
/// (`rclc_executor_add_subscription`): the executor owns the subscriber, so
/// before that there is nothing to register on and this answers
/// `RCL_RET_NOT_INIT`. Otherwise the same returns as
/// [`rclc_publisher_set_matched_callback`].
///
/// # Safety
/// `sub` must point to a valid `rcl_subscription_t`; `user_context` must
/// outlive it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_matched_callback(
    sub: *mut rcl_subscription_t,
    cb: rclc_event_subscription_matched_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            subscription_set(
                sub,
                nros_rmw::EventKind::SubscriptionMatched,
                0,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (sub, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

/// Register a callback for `NROS_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE` —
/// issue 1972. Fires when a publisher on this topic NAME with a different
/// type is seen. Same contract as [`rclc_subscription_set_matched_callback`].
///
/// # Safety
/// See [`rclc_subscription_set_matched_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_incompatible_type_callback(
    sub: *mut rcl_subscription_t,
    cb: rclc_event_subscription_incompatible_type_cb_t,
    user_context: *mut c_void,
) -> rcl_ret_t {
    #[cfg(feature = "rmw-cffi")]
    {
        let cb = cb.map_or(core::ptr::null(), |f| f as *const ());
        unsafe {
            subscription_set(
                sub,
                nros_rmw::EventKind::SubscriptionIncompatibleType,
                0,
                cb,
                user_context,
            )
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (sub, cb, user_context);
        RCL_RET_UNSUPPORTED
    }
}

/// Issue 1972 — the process-wide table that holds a C or C++ status-event
/// registration's `(callback, context, entity)` triple.
///
/// **Why a table.** The backend's channel carries ONE opaque pointer, and
/// neither the C entity (a generated struct users allocate) nor the C++ one
/// (fixed-size storage that RELOCATES on move) has room or a stable address
/// for the triple. A static table has both.
///
/// **Why a TAGGED reference.** What the backend is handed is not a slot's
/// address but `(index, generation)` packed into the pointer. An executor-owned
/// C subscription outlives `rcl_subscription_fini` in the arena, so its backend
/// registration can fire after the slot was released and reused by another
/// entity; the generation makes such a late call a no-op instead of the other
/// entity's callback.
///
/// **The bound.** [`MAX_SLOTS`](status_event_slots::MAX_SLOTS) = 16 live
/// registrations, process-wide, across the C and C++ APIs; no allocation,
/// `no_std`. A publisher can hold four (liveliness lost, offered deadline,
/// matched, incompatible type) and a subscription five (liveliness changed,
/// requested deadline, message lost, matched, incompatible type), so one of
/// each with every kind registered takes nine. A registration past the bound
/// is refused — `RCLC_RET_FULL` in C, `NROS_CPP_RET_FULL` in C++ — never
/// dropped; every setter that can return it says so. Each slot is six
/// pointer-sized words, which is why the table is not sized for the worst case
/// of every entity in an image (issue 1993).
#[cfg(feature = "rmw-cffi")]
pub mod status_event_slots {
    use core::ffi::c_void;
    use portable_atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};

    use nros_rmw::{EventKind, EventPayload, TransportError};

    /// Live registrations, process-wide, across both the C and C++ APIs. Nine
    /// event kinds carry a slot (issue 1993): up to four per publisher, five
    /// per subscription. See the module doc for why 16.
    pub const MAX_SLOTS: usize = 16;

    // One publisher and one subscription with every kind registered must fit.
    const _: () = assert!(MAX_SLOTS >= 4 + 5);
    // The tag packs the index into 8 bits.
    const _: () = assert!(MAX_SLOTS <= 0x100);

    struct Slot {
        /// The registering entity's RMW handle address; 0 = free.
        owner: AtomicUsize,
        kind: AtomicU32,
        generation: AtomicU32,
        entity: AtomicPtr<c_void>,
        cb: AtomicPtr<()>,
        ctx: AtomicPtr<c_void>,
    }

    impl Slot {
        const fn new() -> Self {
            Self {
                owner: AtomicUsize::new(0),
                kind: AtomicU32::new(u32::MAX),
                generation: AtomicU32::new(0),
                entity: AtomicPtr::new(core::ptr::null_mut()),
                cb: AtomicPtr::new(core::ptr::null_mut()),
                ctx: AtomicPtr::new(core::ptr::null_mut()),
            }
        }
    }

    static SLOTS: [Slot; MAX_SLOTS] = [const { Slot::new() }; MAX_SLOTS];

    /// `(index, generation)` packed into a non-null pointer-sized tag. The
    /// index takes the low 8 bits; `+1` keeps the tag non-null.
    fn tag(index: usize, generation: u32) -> *mut c_void {
        ((((generation as usize) << 8) | index) + 1) as *mut c_void
    }

    fn untag(t: *mut c_void) -> Option<(usize, u32)> {
        let v = (t as usize).checked_sub(1)?;
        let index = v & 0xff;
        (index < MAX_SLOTS).then_some((index, (v >> 8) as u32))
    }

    /// The trampoline every slot registration hands the backend.
    unsafe extern "C" fn trampoline(kind: EventKind, payload: *const c_void, t: *mut c_void) {
        let Some((index, generation)) = untag(t) else {
            return;
        };
        let slot = &SLOTS[index];
        if slot.generation.load(Ordering::Acquire) != generation
            || slot.owner.load(Ordering::Acquire) == 0
        {
            return; // a registration that outlived its slot
        }
        if slot.kind.load(Ordering::Acquire) != kind as u32 {
            // The callback's SHAPE was chosen by the registered kind; calling
            // it with another kind's payload would be the wrong signature.
            return;
        }
        let cb = slot.cb.load(Ordering::Acquire);
        if cb.is_null() || payload.is_null() {
            return;
        }
        let entity = slot.entity.load(Ordering::Acquire);
        let ctx = slot.ctx.load(Ordering::Acquire);
        // SAFETY: the backend hands a payload of the kind it names.
        match unsafe { nros_rmw::payload_from_raw(kind, payload) } {
            EventPayload::PublicationMatched(s) | EventPayload::SubscriptionMatched(s) => {
                // SAFETY: registered for a matched kind, so `cb` is a callback
                // taking `(entity*, matched status by value, void*)`; the C and
                // C++ status structs are layout-identical to `MatchedStatus`
                // (const-asserted beside each).
                let f: unsafe extern "C" fn(*mut c_void, nros_rmw::MatchedStatus, *mut c_void) =
                    unsafe { core::mem::transmute(cb) };
                unsafe { f(entity, *s, ctx) };
            }
            EventPayload::PublisherIncompatibleType(s)
            | EventPayload::SubscriptionIncompatibleType(s) => {
                // SAFETY: as above, for the incompatible-type shape.
                let f: unsafe extern "C" fn(
                    *mut c_void,
                    nros_rmw::IncompatibleTypeStatus,
                    *mut c_void,
                ) = unsafe { core::mem::transmute(cb) };
                unsafe { f(entity, *s, ctx) };
            }
            // Issue 1993 — the Tier-1 kinds. No `_` arm: a payload variant
            // added later must be routed here, not dropped without a trace.
            EventPayload::LivelinessChanged(s) => {
                // SAFETY: registered for `LivelinessChanged`, so `cb` takes
                // `(entity*, liveliness status by value, void*)`; the C and
                // C++ status structs are layout-identical to
                // `LivelinessChangedStatus` (const-asserted beside each).
                let f: unsafe extern "C" fn(
                    *mut c_void,
                    nros_rmw::LivelinessChangedStatus,
                    *mut c_void,
                ) = unsafe { core::mem::transmute(cb) };
                unsafe { f(entity, *s, ctx) };
            }
            EventPayload::RequestedDeadlineMissed(s)
            | EventPayload::MessageLost(s)
            | EventPayload::LivelinessLost(s)
            | EventPayload::OfferedDeadlineMissed(s) => {
                // SAFETY: as above, for the count shape every one of these
                // four kinds shares (`CountStatus`).
                let f: unsafe extern "C" fn(*mut c_void, nros_rmw::CountStatus, *mut c_void) =
                    unsafe { core::mem::transmute(cb) };
                unsafe { f(entity, *s, ctx) };
            }
        }
    }

    /// Record `(cb, ctx, entity)` for `(owner, kind)` and hand the backend a
    /// tagged reference to it through `backend(kind, trampoline, tag)`.
    ///
    /// A second registration for the same `(owner, kind)` reuses the slot. On
    /// a backend refusal a freshly claimed slot is released again.
    ///
    /// # Safety
    /// `cb` must be the callback shape `kind` selects (see the trampoline);
    /// `owner` must identify a live RMW handle until [`release_owner`].
    pub unsafe fn register(
        owner: usize,
        entity: *mut c_void,
        kind: EventKind,
        cb: *const (),
        ctx: *mut c_void,
        backend: impl FnOnce(
            EventKind,
            nros_rmw::EventCallback,
            *mut c_void,
        ) -> Result<(), TransportError>,
    ) -> Result<(), TransportError> {
        if owner == 0 || cb.is_null() {
            return Err(TransportError::InvalidArgument);
        }
        let k = kind as u32;
        let mut fresh = false;
        let index = match SLOTS.iter().position(|s| {
            s.owner.load(Ordering::Acquire) == owner && s.kind.load(Ordering::Acquire) == k
        }) {
            Some(i) => i,
            None => {
                let Some(i) = SLOTS.iter().position(|s| {
                    s.owner
                        .compare_exchange(0, owner, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                }) else {
                    return Err(TransportError::BadAlloc);
                };
                fresh = true;
                let s = &SLOTS[i];
                s.kind.store(k, Ordering::Release);
                s.generation.fetch_add(1, Ordering::AcqRel);
                i
            }
        };
        let s = &SLOTS[index];
        s.entity.store(entity, Ordering::Release);
        s.ctx.store(ctx, Ordering::Release);
        s.cb.store(cb as *mut (), Ordering::Release);
        let t = tag(index, s.generation.load(Ordering::Acquire));
        match backend(kind, trampoline, t) {
            Ok(()) => Ok(()),
            Err(e) => {
                if fresh {
                    clear(s);
                }
                Err(e)
            }
        }
    }

    fn clear(s: &Slot) {
        s.cb.store(core::ptr::null_mut(), Ordering::Release);
        s.ctx.store(core::ptr::null_mut(), Ordering::Release);
        s.entity.store(core::ptr::null_mut(), Ordering::Release);
        s.kind.store(u32::MAX, Ordering::Release);
        // Bump BEFORE freeing, so a late backend call with the old tag misses.
        s.generation.fetch_add(1, Ordering::AcqRel);
        s.owner.store(0, Ordering::Release);
    }

    /// Release every slot `owner` holds. Called when the entity is destroyed.
    pub fn release_owner(owner: usize) {
        if owner == 0 {
            return;
        }
        for s in SLOTS.iter() {
            if s.owner.load(Ordering::Acquire) == owner {
                clear(s);
            }
        }
    }

    /// An entity RELOCATED (C++ move): its slots follow it to `new_owner`, and
    /// the callback now receives `new_entity`. The backend's tag is unchanged —
    /// it names the slot, not the entity.
    pub fn rekey(old_owner: usize, new_owner: usize, new_entity: *mut c_void) {
        if old_owner == 0 || new_owner == 0 || old_owner == new_owner {
            return;
        }
        for s in SLOTS.iter() {
            if s.owner.load(Ordering::Acquire) == old_owner {
                s.entity.store(new_entity, Ordering::Release);
                s.owner.store(new_owner, Ordering::Release);
            }
        }
    }

    /// How many slots are in use — for tests.
    pub fn in_use() -> usize {
        SLOTS
            .iter()
            .filter(|s| s.owner.load(Ordering::Acquire) != 0)
            .count()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use core::sync::atomic::{AtomicI32, AtomicU32 as StdAtomicU32};

        static SEEN_CURRENT: StdAtomicU32 = StdAtomicU32::new(0);
        static SEEN_CHANGE: AtomicI32 = AtomicI32::new(0);
        static SEEN_ENTITY: AtomicUsize = AtomicUsize::new(0);
        static CALLS: StdAtomicU32 = StdAtomicU32::new(0);

        unsafe extern "C" fn user_matched(
            entity: *mut c_void,
            s: nros_rmw::MatchedStatus,
            ctx: *mut c_void,
        ) {
            assert_eq!(ctx as usize, 0xC0);
            SEEN_ENTITY.store(entity as usize, Ordering::SeqCst);
            SEEN_CURRENT.store(s.current_count, Ordering::SeqCst);
            SEEN_CHANGE.store(s.current_count_change, Ordering::SeqCst);
            CALLS.fetch_add(1, Ordering::SeqCst);
        }

        static LIVE_ALIVE: StdAtomicU32 = StdAtomicU32::new(0);
        static LIVE_NOT_ALIVE_CHANGE: AtomicI32 = AtomicI32::new(0);
        static COUNT_TOTAL: StdAtomicU32 = StdAtomicU32::new(0);
        static COUNT_CALLS: StdAtomicU32 = StdAtomicU32::new(0);

        unsafe extern "C" fn user_liveliness(
            entity: *mut c_void,
            s: nros_rmw::LivelinessChangedStatus,
            ctx: *mut c_void,
        ) {
            assert_eq!(ctx as usize, 0xC1);
            SEEN_ENTITY.store(entity as usize, Ordering::SeqCst);
            LIVE_ALIVE.store(s.alive_count as u32, Ordering::SeqCst);
            LIVE_NOT_ALIVE_CHANGE.store(s.not_alive_count_change as i32, Ordering::SeqCst);
        }

        unsafe extern "C" fn user_count(
            entity: *mut c_void,
            s: nros_rmw::CountStatus,
            ctx: *mut c_void,
        ) {
            assert_eq!(ctx as usize, 0xC2);
            SEEN_ENTITY.store(entity as usize, Ordering::SeqCst);
            COUNT_TOTAL.store(s.total_count, Ordering::SeqCst);
            COUNT_CALLS.fetch_add(1, Ordering::SeqCst);
        }

        /// Issue 1993 — the Tier-1 shapes reach their callbacks, a payload of
        /// another kind is refused, and one publisher plus one subscription
        /// with every kind registered fit in the table. Called from the one
        /// test below (the table is process-global).
        fn tier1_kinds_dispatch_and_fit() {
            let mut regs: [Option<(nros_rmw::EventCallback, *mut c_void)>; 9] = [None; 9];
            let pub_kinds = [
                (EventKind::LivelinessLost, user_count as *const ()),
                (EventKind::OfferedDeadlineMissed, user_count as *const ()),
                (EventKind::PublicationMatched, user_matched as *const ()),
                (
                    EventKind::PublisherIncompatibleType,
                    user_matched as *const (),
                ),
            ];
            let sub_kinds = [
                (EventKind::LivelinessChanged, user_liveliness as *const ()),
                (EventKind::RequestedDeadlineMissed, user_count as *const ()),
                (EventKind::MessageLost, user_count as *const ()),
                (EventKind::SubscriptionMatched, user_matched as *const ()),
                (
                    EventKind::SubscriptionIncompatibleType,
                    user_matched as *const (),
                ),
            ];
            for (owner, entity, kinds) in [
                (0x5000usize, 0xA1usize, &pub_kinds[..]),
                (0x6000, 0xA2, &sub_kinds[..]),
            ] {
                for (kind, cb) in kinds {
                    let ctx = match kind {
                        EventKind::LivelinessChanged => 0xC1,
                        EventKind::PublicationMatched
                        | EventKind::SubscriptionMatched
                        | EventKind::PublisherIncompatibleType
                        | EventKind::SubscriptionIncompatibleType => 0xC0,
                        _ => 0xC2,
                    };
                    unsafe {
                        register(
                            owner,
                            entity as *mut c_void,
                            *kind,
                            *cb,
                            ctx as *mut c_void,
                            |k, f, t| {
                                regs[k as usize] = Some((f, t));
                                Ok(())
                            },
                        )
                    }
                    .expect("every kind of one publisher and one subscription fits");
                }
            }
            assert_eq!(in_use(), 9);

            let (f, t) = regs[EventKind::LivelinessChanged as usize].unwrap();
            let live = nros_rmw::LivelinessChangedStatus {
                alive_count: 3,
                not_alive_count: 1,
                alive_count_change: 0,
                not_alive_count_change: -1,
            };
            unsafe {
                f(
                    EventKind::LivelinessChanged,
                    &live as *const _ as *const c_void,
                    t,
                )
            };
            assert_eq!(SEEN_ENTITY.load(Ordering::SeqCst), 0xA2);
            assert_eq!(LIVE_ALIVE.load(Ordering::SeqCst), 3);
            assert_eq!(LIVE_NOT_ALIVE_CHANGE.load(Ordering::SeqCst), -1);

            let count = nros_rmw::CountStatus {
                total_count: 42,
                total_count_change: 1,
            };
            for (kind, entity) in [
                (EventKind::RequestedDeadlineMissed, 0xA2),
                (EventKind::MessageLost, 0xA2),
                (EventKind::LivelinessLost, 0xA1),
                (EventKind::OfferedDeadlineMissed, 0xA1),
            ] {
                let before = COUNT_CALLS.load(Ordering::SeqCst);
                let (f, t) = regs[kind as usize].unwrap();
                unsafe { f(kind, &count as *const _ as *const c_void, t) };
                assert_eq!(COUNT_CALLS.load(Ordering::SeqCst), before + 1, "{kind:?}");
                assert_eq!(SEEN_ENTITY.load(Ordering::SeqCst), entity, "{kind:?}");
                assert_eq!(COUNT_TOTAL.load(Ordering::SeqCst), 42, "{kind:?}");
            }

            // A payload of another kind through a slot is refused: the
            // callback's shape was chosen by the REGISTERED kind.
            let before = COUNT_CALLS.load(Ordering::SeqCst);
            let (f, t) = regs[EventKind::MessageLost as usize].unwrap();
            unsafe {
                f(
                    EventKind::LivelinessChanged,
                    &live as *const _ as *const c_void,
                    t,
                )
            };
            assert_eq!(COUNT_CALLS.load(Ordering::SeqCst), before);

            release_owner(0x5000);
            release_owner(0x6000);
            assert_eq!(in_use(), 0);
        }

        /// One test, because the table is process-global and tests run in
        /// parallel threads.
        #[test]
        fn slots_route_tag_relocate_and_refuse_stale_calls() {
            let mut captured: Option<(nros_rmw::EventCallback, *mut c_void)> = None;
            let r = unsafe {
                register(
                    0x1000,
                    0xE1 as *mut c_void,
                    EventKind::SubscriptionMatched,
                    user_matched as *const (),
                    0xC0 as *mut c_void,
                    |k, f, t| {
                        assert_eq!(k, EventKind::SubscriptionMatched);
                        captured = Some((f, t));
                        Ok(())
                    },
                )
            };
            assert!(r.is_ok());
            assert_eq!(in_use(), 1);
            let (f, t) = captured.expect("backend saw the registration");
            let st = nros_rmw::MatchedStatus {
                total_count: 1,
                total_count_change: 1,
                current_count: 1,
                current_count_change: 1,
            };
            unsafe {
                f(
                    EventKind::SubscriptionMatched,
                    &st as *const _ as *const c_void,
                    t,
                )
            };
            assert_eq!(CALLS.load(Ordering::SeqCst), 1);
            assert_eq!(SEEN_ENTITY.load(Ordering::SeqCst), 0xE1);
            assert_eq!(SEEN_CURRENT.load(Ordering::SeqCst), 1);

            // Relocation: the same backend tag now reaches the NEW entity.
            rekey(0x1000, 0x2000, 0xE2 as *mut c_void);
            let left = nros_rmw::MatchedStatus {
                total_count: 1,
                total_count_change: 0,
                current_count: 0,
                current_count_change: -1,
            };
            unsafe {
                f(
                    EventKind::SubscriptionMatched,
                    &left as *const _ as *const c_void,
                    t,
                )
            };
            assert_eq!(CALLS.load(Ordering::SeqCst), 2);
            assert_eq!(SEEN_ENTITY.load(Ordering::SeqCst), 0xE2);
            assert_eq!(SEEN_CHANGE.load(Ordering::SeqCst), -1);

            // Released: a registration that outlived its slot is a no-op, even
            // after another entity claims the same slot.
            release_owner(0x2000);
            assert_eq!(in_use(), 0);
            let mut captured2 = None;
            unsafe {
                register(
                    0x3000,
                    0xE3 as *mut c_void,
                    EventKind::SubscriptionMatched,
                    user_matched as *const (),
                    0xC0 as *mut c_void,
                    |_, f, t| {
                        captured2 = Some((f, t));
                        Ok(())
                    },
                )
            }
            .expect("re-register");
            unsafe {
                f(
                    EventKind::SubscriptionMatched,
                    &st as *const _ as *const c_void,
                    t,
                )
            };
            assert_eq!(
                CALLS.load(Ordering::SeqCst),
                2,
                "the stale tag must not fire"
            );
            let (f2, t2) = captured2.unwrap();
            assert_ne!(t, t2, "a reused slot carries a new generation");
            unsafe {
                f2(
                    EventKind::SubscriptionMatched,
                    &st as *const _ as *const c_void,
                    t2,
                )
            };
            assert_eq!(SEEN_ENTITY.load(Ordering::SeqCst), 0xE3);
            release_owner(0x3000);

            // A backend refusal leaves no slot behind.
            let r = unsafe {
                register(
                    0x4000,
                    core::ptr::null_mut(),
                    EventKind::PublicationMatched,
                    user_matched as *const (),
                    core::ptr::null_mut(),
                    |_, _, _| Err(TransportError::Unsupported),
                )
            };
            assert_eq!(r, Err(TransportError::Unsupported));
            assert_eq!(in_use(), 0);

            tier1_kinds_dispatch_and_fit();

            // The table is bounded, and says so.
            for i in 0..MAX_SLOTS {
                unsafe {
                    register(
                        0x10_000 + i,
                        core::ptr::null_mut(),
                        EventKind::PublicationMatched,
                        user_matched as *const (),
                        core::ptr::null_mut(),
                        |_, _, _| Ok(()),
                    )
                }
                .expect("fits");
            }
            let full = unsafe {
                register(
                    0x20_000,
                    core::ptr::null_mut(),
                    EventKind::PublicationMatched,
                    user_matched as *const (),
                    core::ptr::null_mut(),
                    |_, _, _| Ok(()),
                )
            };
            assert_eq!(full, Err(TransportError::BadAlloc));
            for i in 0..MAX_SLOTS {
                release_owner(0x10_000 + i);
            }
            assert_eq!(in_use(), 0);
        }
    }
}
