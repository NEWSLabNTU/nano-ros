//! Subscription FFI functions for the C++ API.

use core::ffi::{c_char, c_void};

use nros_node::config::DEFAULT_RX_BUF_SIZE;
use nros_node::limits::MAX_TOPIC_LEN;
use nros_rmw::{Session, Subscriber as SubscriberTrait, TopicInfo};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_OK, NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t,
    nros_cpp_ret_t,
};

/// Subscription wrapper stored in caller-provided inline storage.
pub(crate) struct CppSubscription {
    handle: nros::internals::RmwSubscriber,
    buffer: [u8; DEFAULT_RX_BUF_SIZE],
    topic_name: [u8; MAX_TOPIC_LEN],
    topic_name_len: usize,
}

// CPP_SUBSCRIPTION_OPAQUE_U64S is computed from size_of::<CppSubscription>() — always exact.

/// Create a subscription on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `CPP_SUBSCRIPTION_OPAQUE_U64S * 8` bytes, aligned to 8 bytes.
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// 8-byte-aligned buffer of at least `CPP_SUBSCRIPTION_OPAQUE_U64S * 8` bytes.
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

    // Extract node name/namespace from the node handle
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
            let mut sub_handle = CppSubscription {
                handle,
                buffer: [0u8; DEFAULT_RX_BUF_SIZE],
                topic_name: [0u8; MAX_TOPIC_LEN],
                topic_name_len: topic_str.len().min(MAX_TOPIC_LEN - 1),
            };
            sub_handle.topic_name[..sub_handle.topic_name_len]
                .copy_from_slice(&topic_str.as_bytes()[..sub_handle.topic_name_len]);

            // Write directly into caller-provided storage (no heap allocation)
            unsafe {
                core::ptr::write(storage as *mut CppSubscription, sub_handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive raw CDR data from a subscription (non-blocking).
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

    let sub = unsafe { &mut *(storage as *mut CppSubscription) };

    match sub.handle.try_recv_raw(&mut sub.buffer) {
        Ok(Some(len)) => {
            if len <= out_capacity {
                unsafe {
                    core::ptr::copy_nonoverlapping(sub.buffer.as_ptr(), out_data, len);
                    *out_len = len;
                }
                NROS_CPP_RET_OK
            } else {
                unsafe {
                    *out_len = len;
                }
                NROS_CPP_RET_FULL
            }
        }
        Ok(None) => {
            unsafe {
                *out_len = 0;
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
        core::ptr::drop_in_place(storage as *mut CppSubscription);
    }
    NROS_CPP_RET_OK
}

/// Relocate a `CppSubscription` from `old_storage` to `new_storage`.
///
/// Subscriptions are pull-based (`try_recv_raw`) and register nothing
/// externally that references the storage address — relocation is a
/// straight `ptr::read` + `ptr::write`. Called by the C++ `Subscription`
/// move ctor / move assignment.
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
        let value = core::ptr::read(old_storage as *mut CppSubscription);
        core::ptr::write(new_storage as *mut CppSubscription, value);
    }
    NROS_CPP_RET_OK
}

/// Get the topic name of a subscription.
///
/// # Safety
/// `storage` must be a valid subscription storage, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_get_topic_name(
    storage: *const c_void,
) -> *const c_char {
    if storage.is_null() {
        return core::ptr::null();
    }
    let sub = unsafe { &*(storage as *const CppSubscription) };
    sub.topic_name.as_ptr() as *const c_char
}
