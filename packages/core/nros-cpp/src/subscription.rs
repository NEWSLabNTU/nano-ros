//! Subscription FFI functions for the C++ API.
//!
//! Phase 87.6 (thin-wrapper refactor): caller's opaque storage holds a
//! bare `RmwSubscriber` handle. Topic name lives on the C++
//! `nros::Subscription<M>` class. Received CDR bytes are copied directly
//! into the caller's output buffer — no Rust-side 1 KiB scratch buffer.

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

    match ctx
        .executor
        .session_mut()
        .create_subscriber(&topic_info, qos_settings)
    {
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
/// — no Rust-side scratch. If the message is larger than `out_capacity`
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
