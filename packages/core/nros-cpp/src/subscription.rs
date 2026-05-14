//! Subscription FFI functions for the C++ API.
//!
//! Phase 87.6 (thin-wrapper refactor): caller's opaque storage holds a
//! bare `RmwSubscriber` handle. Topic name lives on the C++
//! `nros::Subscription<M>` class. Received CDR bytes are copied directly
//! into the caller's output buffer — no runtime 1 KiB scratch buffer.

use core::ffi::{c_char, c_void};

use nros_rmw::{Session, Subscriber as SubscriberTrait, TopicInfo, TransportError};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_OK, NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t,
    nros_cpp_ret_t,
};

/// Create a subscription on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<RmwSubscriber>()` bytes (exposed via `NROS_SUBSCRIBER_SIZE`).
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// appropriately-aligned buffer of at least `NROS_SUBSCRIBER_SIZE` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_create(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || topic.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || storage.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let node_ref = unsafe { &*node };
    if node_ref.executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let topic_str = match unsafe { cstr_to_str(topic) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let node_name_str = core::str::from_utf8(&node_ref.name)
        .ok()
        .and_then(|s| s.split('\0').next());
    let ns_str = core::str::from_utf8(&node_ref.namespace)
        .ok()
        .and_then(|s| s.split('\0').next())
        .unwrap_or("/");

    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };

    let topic_info = TopicInfo::new(topic_str, type_str, hash_str)
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str);
    let topic_info = match node_name_str {
        Some(name) if !name.is_empty() => topic_info.with_node_name(name),
        _ => topic_info,
    };

    let qos_settings = qos.to_qos_settings();

    // Phase 104.C.9.b — when the Node was created via
    // `nros_cpp_node_create_ex` (multi-RMW path), `node.node_id != 0`
    // and the subscriber must land on the Node's bound session, not
    // the executor's primary. `node_session_mut` resolves both cases
    // transparently.
    let session = if node_ref.node_id != 0 {
        match ctx
            .executor
            .node_session_mut(nros_node::executor::NodeId::from_raw(node_ref.node_id))
        {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    } else {
        ctx.executor.session_mut()
    };

    match session.create_subscriber(&topic_info, qos_settings) {
        Ok(handle) => {
            unsafe {
                core::ptr::write(storage as *mut nros::internals::RmwSubscriber, handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive raw CDR data from a subscription (non-blocking).
///
/// Writes the received CDR bytes directly into the caller's output buffer
/// — no runtime scratch. If the message is larger than `out_capacity`
/// the backend drops it and returns `NROS_CPP_RET_FULL`; callers that need
/// to handle oversized messages should size `out_data` to the message type's
/// `SERIALIZED_SIZE_MAX` (exactly what `Subscription<M>::try_recv` does).
///
/// # Safety
/// `storage` must be a valid subscription storage. `out_data` must point to
/// `out_capacity` writable bytes. `out_len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_try_recv_raw(
    storage: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || out_data.is_null() || out_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    let out_slice = unsafe { core::slice::from_raw_parts_mut(out_data, out_capacity) };

    match sub.try_recv_raw(out_slice) {
        Ok(Some(len)) => {
            unsafe {
                *out_len = len;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe {
                *out_len = 0;
            }
            NROS_CPP_RET_OK
        }
        Err(TransportError::BufferTooSmall | TransportError::MessageTooLarge) => {
            // The backend drops the oversized message; `out_len` stays 0
            // because the backend doesn't report the actual length.
            unsafe {
                *out_len = 0;
            }
            NROS_CPP_RET_FULL
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

// ============================================================================
// Phase 124.A.7 — zero-copy subscription borrow / release
// ============================================================================

/// Phase 124.A.7 — borrow the next message in place.
///
/// On success, `*out_buf` points at `*out_len` bytes (read-only) until
/// the caller calls `nros_cpp_subscription_release(storage, token)`.
///
/// Returns `> 0` (length), `0` (no message), or negative error code.
///
/// # Safety
/// All pointer parameters must be valid. Only one outstanding borrow
/// per subscription is allowed.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_borrow(
    storage: *mut c_void,
    out_buf: *mut *const u8,
    out_len: *mut usize,
    out_token: *mut *mut c_void,
) -> i32 {
    if storage.is_null() || out_buf.is_null() || out_len.is_null() || out_token.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    use nros_rmw::SlotBorrowing;
    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    match sub.try_borrow() {
        Ok(Some(view)) => {
            let buf_ptr = view.as_ref().as_ptr();
            let len = view.as_ref().len();
            // SAFETY: erase lifetime — caller's release contract ensures
            // the view doesn't outlive the subscription.
            let view_static: nros::internals::RmwView<'static> = unsafe {
                core::mem::transmute::<
                    nros::internals::RmwView<'_>,
                    nros::internals::RmwView<'static>,
                >(view)
            };
            let boxed = alloc::boxed::Box::new(view_static);
            unsafe {
                *out_buf = buf_ptr;
                *out_len = len;
                *out_token = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            len as i32
        }
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Phase 124.A.7 — release a previously borrowed view.
///
/// # Safety
/// `storage` must be the subscription the token was borrowed from.
/// `token` must come from a matching `nros_cpp_subscription_borrow`
/// and must not be reused after this call.
#[cfg(feature = "lending")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_release(
    storage: *mut c_void,
    token: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() || token.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let _view: alloc::boxed::Box<nros::internals::RmwView<'static>> = unsafe {
        alloc::boxed::Box::from_raw(token as *mut nros::internals::RmwView<'static>)
    };
    NROS_CPP_RET_OK
}

/// Phase 124.D.1 — burst-take.
///
/// Drain up to `max_msgs` queued samples into the contiguous `buf`
/// block in a single call. The i-th delivered sample lives at
/// `buf + i * per_msg_cap` with length `out_lens[i]`. Returns the
/// number of samples delivered (`>= 0`) via `out_count` and an
/// `nros_cpp_ret_t` status:
///   * `NROS_CPP_RET_OK` — `*out_count` was written.
///   * `NROS_CPP_RET_INVALID_ARGUMENT` — null pointer or zero
///     per-message cap.
///   * `NROS_CPP_RET_ERROR` — backend-level transport failure.
///
/// # Safety
/// `storage` must be a valid initialized subscription. `buf` must
/// point to a writable block of `max_msgs * per_msg_cap` bytes.
/// `out_lens` must point to a writable array of `max_msgs` `size_t`
/// slots. `out_count` must be a writable `usize` pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_try_recv_sequence(
    storage: *mut c_void,
    buf: *mut u8,
    per_msg_cap: usize,
    max_msgs: usize,
    out_lens: *mut usize,
    out_count: *mut usize,
) -> nros_cpp_ret_t {
    use nros_rmw::Subscriber;
    if storage.is_null() || buf.is_null() || out_lens.is_null() || out_count.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    if per_msg_cap == 0 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let sub = unsafe { &mut *(storage as *mut nros::internals::RmwSubscriber) };
    let buf_slice = unsafe {
        core::slice::from_raw_parts_mut(buf, max_msgs.saturating_mul(per_msg_cap))
    };
    let lens_slice = unsafe { core::slice::from_raw_parts_mut(out_lens, max_msgs) };
    match sub.try_recv_sequence(buf_slice, per_msg_cap, max_msgs, lens_slice) {
        Ok(count) => {
            unsafe {
                *out_count = count;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Destroy a subscription (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized subscription storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut nros::internals::RmwSubscriber);
    }
    NROS_CPP_RET_OK
}

/// Relocate an `RmwSubscriber` from `old_storage` to `new_storage`.
///
/// Subscriptions are pull-based (`try_recv_raw`) and register nothing
/// externally that references the storage address — relocation is a
/// straight `ptr::read` + `ptr::write`.
///
/// # Safety
/// See `nros_cpp_publisher_relocate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut nros::internals::RmwSubscriber);
        core::ptr::write(new_storage as *mut nros::internals::RmwSubscriber, value);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Phase 108 — status events (stub: returns NROS_CPP_RET_UNSUPPORTED)
// ============================================================================
//
// User-facing C++ event-setter shims. Match the typedefs in
// `<nros/subscription.hpp>`. Backend wiring lands per phase (109+);
// for now the runtime returns NROS_CPP_RET_UNSUPPORTED so the C++
// API compiles and is callable.

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_liveliness_changed_status_t {
    pub alive_count: u16,
    pub not_alive_count: u16,
    pub alive_count_change: i16,
    pub not_alive_count_change: i16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_count_status_t {
    pub total_count: u32,
    pub total_count_change: u32,
}

pub type nros_cpp_liveliness_changed_cb_t = Option<
    unsafe extern "C" fn(
        storage: *mut c_void,
        status: nros_cpp_liveliness_changed_status_t,
        user_context: *mut c_void,
    ),
>;

pub type nros_cpp_subscriber_count_cb_t = Option<
    unsafe extern "C" fn(
        storage: *mut c_void,
        status: nros_cpp_count_status_t,
        user_context: *mut c_void,
    ),
>;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_liveliness_changed(
    _storage: *mut c_void,
    _cb: nros_cpp_liveliness_changed_cb_t,
    _user_context: *mut c_void,
) -> nros_cpp_ret_t {
    crate::NROS_CPP_RET_UNSUPPORTED
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_requested_deadline_missed(
    _storage: *mut c_void,
    _deadline_ms: u32,
    _cb: nros_cpp_subscriber_count_cb_t,
    _user_context: *mut c_void,
) -> nros_cpp_ret_t {
    crate::NROS_CPP_RET_UNSUPPORTED
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_message_lost(
    _storage: *mut c_void,
    _cb: nros_cpp_subscriber_count_cb_t,
    _user_context: *mut c_void,
) -> nros_cpp_ret_t {
    crate::NROS_CPP_RET_UNSUPPORTED
}
