//! Subscription FFI functions for the C++ API.

use core::ffi::{c_char, c_void};

use nros_rmw::{Session, Subscriber as SubscriberTrait, TopicInfo};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_OK, NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t,
    nros_cpp_ret_t,
};

/// Default receive buffer size (matches nros-node's DEFAULT_RX_BUF_SIZE).
const RX_BUF_SIZE: usize = 1024;

/// Boxed subscription handle stored behind `void*`.
struct CppSubscription {
    handle: nros::internals::RmwSubscriber,
    buffer: [u8; RX_BUF_SIZE],
    topic_name: [u8; 256],
    topic_name_len: usize,
}

/// Create a subscription on a node.
///
/// # Parameters
/// * `node` — Node handle from `nros_cpp_node_create()`.
/// * `topic` — Topic name (null-terminated).
/// * `type_name` — ROS message type name (null-terminated).
/// * `type_hash` — ROS type hash string (null-terminated).
/// * `qos` — QoS settings.
/// * `out_handle` — Receives the opaque subscription handle on success.
///
/// # Safety
/// All pointer parameters must be valid. `out_handle` must point to a `*mut c_void`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_create(
    node: *const nros_cpp_node_t,
    topic: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    out_handle: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || topic.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || out_handle.is_null()
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
                buffer: [0u8; RX_BUF_SIZE],
                topic_name: [0u8; 256],
                topic_name_len: topic_str.len().min(255),
            };
            sub_handle.topic_name[..sub_handle.topic_name_len]
                .copy_from_slice(&topic_str.as_bytes()[..sub_handle.topic_name_len]);

            let boxed = alloc::boxed::Box::new(sub_handle);
            unsafe {
                *out_handle = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive raw CDR data from a subscription (non-blocking).
///
/// Receives into an internal buffer, then copies to the caller's buffer.
///
/// # Parameters
/// * `handle` — Subscription handle from `nros_cpp_subscription_create()`.
/// * `out_data` — Caller's buffer to receive CDR data.
/// * `out_capacity` — Size of the caller's buffer.
/// * `out_len` — Receives the number of bytes written (0 if no data available).
///
/// # Returns
/// * `NROS_CPP_RET_OK` — Data received and copied, or no data available (`*out_len == 0`).
/// * `NROS_CPP_RET_FULL` — Data received but caller's buffer too small.
/// * `NROS_CPP_RET_ERROR` — Transport error.
///
/// # Safety
/// `handle` must be a valid subscription handle. `out_data` must point to `out_capacity`
/// writable bytes. `out_len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_try_recv_raw(
    handle: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || out_data.is_null() || out_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let sub = unsafe { &mut *(handle as *mut CppSubscription) };

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

/// Destroy a subscription and free its resources.
///
/// # Safety
/// `handle` must be a valid subscription handle, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_destroy(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        let _sub = alloc::boxed::Box::from_raw(handle as *mut CppSubscription);
    }
    // subscription dropped here
    NROS_CPP_RET_OK
}

/// Get the topic name of a subscription.
///
/// Returns a pointer to the null-terminated topic name string stored in the
/// subscription handle. The pointer is valid as long as the subscription is alive.
///
/// # Safety
/// `handle` must be a valid subscription handle, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_get_topic_name(
    handle: *const c_void,
) -> *const c_char {
    if handle.is_null() {
        return core::ptr::null();
    }
    let sub = unsafe { &*(handle as *const CppSubscription) };
    sub.topic_name.as_ptr() as *const c_char
}
