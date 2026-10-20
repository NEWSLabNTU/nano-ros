//! Phase 108 — status-event API for the nros C user-facing surface.
//!
//! User-facing typedefs + setter functions for status events. The five
//! Tier-1 setters (liveliness, deadline, message-lost) still return
//! `RCL_RET_UNSUPPORTED` at this layer; the four issue-1972 setters
//! (matched, incompatible type) reach the backend through
//! [`status_event_slots`].

use core::ffi::c_void;

use crate::{error::*, publisher::rcl_publisher_t, subscription::rcl_subscription_t};

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

/// Register a callback for `NROS_EVENT_LIVELINESS_CHANGED`.
///
/// Returns `NROS_RMW_RET_UNSUPPORTED` until the active backend wires
/// up liveliness-event detection (Phase 109+). Applications can call
/// it today; the event fires once the backend gains support.
///
/// # Safety
///
/// `sub` must point to a valid, initialised `rcl_subscription_t`.
/// `cb` must be a valid function pointer or `None`. `user_context`
/// is opaque to nros and must outlive the subscription.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_liveliness_changed_callback(
    _sub: *mut rcl_subscription_t,
    _cb: rclc_event_liveliness_changed_cb_t,
    _user_context: *mut c_void,
) -> rcl_ret_t {
    // Backend wiring lands per-phase (109+); for now, the API surface
    // is exposed but no backend implements the underlying event.
    RCL_RET_UNSUPPORTED
}

/// Register a callback for `NROS_EVENT_REQUESTED_DEADLINE_MISSED`.
///
/// `deadline_ms` is the maximum acceptable inter-arrival time for
/// expected samples. When a sample doesn't arrive within this window,
/// the callback fires.
///
/// # Safety
///
/// See [`rclc_subscription_set_liveliness_changed_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_requested_deadline_missed_callback(
    _sub: *mut rcl_subscription_t,
    _deadline_ms: u32,
    _cb: rclc_event_subscriber_count_cb_t,
    _user_context: *mut c_void,
) -> rcl_ret_t {
    RCL_RET_UNSUPPORTED
}

/// Register a callback for `NROS_EVENT_MESSAGE_LOST`.
///
/// # Safety
///
/// See [`rclc_subscription_set_liveliness_changed_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_subscription_set_message_lost_callback(
    _sub: *mut rcl_subscription_t,
    _cb: rclc_event_subscriber_count_cb_t,
    _user_context: *mut c_void,
) -> rcl_ret_t {
    RCL_RET_UNSUPPORTED
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

/// Register a callback for `NROS_EVENT_LIVELINESS_LOST`.
///
/// # Safety
///
/// `pub_` must point to a valid, initialised `rcl_publisher_t`.
/// `cb` must be a valid function pointer or `None`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_set_liveliness_lost_callback(
    _pub_: *mut rcl_publisher_t,
    _cb: rclc_event_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> rcl_ret_t {
    RCL_RET_UNSUPPORTED
}

/// Register a callback for `NROS_EVENT_OFFERED_DEADLINE_MISSED`.
///
/// `deadline_ms` is the publisher's offered minimum-rate window.
///
/// # Safety
///
/// See [`rclc_publisher_set_liveliness_lost_callback`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_publisher_set_offered_deadline_missed_callback(
    _pub_: *mut rcl_publisher_t,
    _deadline_ms: u32,
    _cb: rclc_event_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> rcl_ret_t {
    RCL_RET_UNSUPPORTED
}

// ============================================================================
// Issue 1972 — matched and incompatible-type events, WIRED
// ============================================================================
//
// Unlike the five Tier-1 setters above, these four reach the backend. The
// difference is a place to keep the user's `(callback, context, entity)`
// triple: the backend's status-event channel carries ONE opaque pointer, and
// the C entity's layout is a generated header users allocate, so the triple
// lives in [`status_event_slots`] — a small static table shared with the C++
// API — and the backend is handed a tagged slot reference instead.

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

/// Register on an inline-storage publisher.
#[cfg(feature = "rmw-cffi")]
unsafe fn publisher_set(
    pub_: *mut rcl_publisher_t,
    kind: nros_rmw::EventKind,
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
    let r = unsafe {
        status_event_slots::register(
            handle as usize,
            pub_ as *mut c_void,
            kind,
            cb,
            user_context,
            |k, tramp, tag| {
                nros_rmw::Publisher::register_event_callback(&mut *handle, k, 0, tramp, tag)
            },
        )
    };
    slot_verdict(r)
}

/// Register on an executor-owned subscription.
#[cfg(feature = "rmw-cffi")]
unsafe fn subscription_set(
    sub: *mut rcl_subscription_t,
    kind: nros_rmw::EventKind,
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
    let exec = unsafe { crate::executor::get_executor(exec_t) };
    let Some(handle) = (unsafe { exec.subscription_handle_mut(subscription.handle_id) }) else {
        return RCL_RET_NOT_INIT;
    };
    let handle = handle as *mut nros::internals::RmwSubscriber;
    let r = unsafe {
        status_event_slots::register(
            handle as usize,
            sub as *mut c_void,
            kind,
            cb,
            user_context,
            |k, tramp, tag| {
                nros_rmw::Subscription::register_event_callback(&mut *handle, k, 0, tramp, tag)
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
/// Bounded (16 slots), no allocation, `no_std`.
#[cfg(feature = "rmw-cffi")]
pub mod status_event_slots {
    use core::ffi::c_void;
    use portable_atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};

    use nros_rmw::{EventKind, EventPayload, TransportError};

    /// Live registrations, process-wide, across both the C and C++ APIs. Four
    /// event kinds per entity carry a slot today (issue 1972).
    const MAX_SLOTS: usize = 16;

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
            _ => {}
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
