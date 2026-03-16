//! Publisher FFI functions for the C++ API.

use core::ffi::{c_char, c_void};

use nros_rmw::{Publisher as PublisherTrait, Session, TopicInfo};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

/// Publisher wrapper stored in caller-provided inline storage.
pub(crate) struct CppPublisher {
    handle: nros::internals::RmwPublisher,
    topic_name: [u8; 256],
    topic_name_len: usize,
}

// CPP_PUBLISHER_OPAQUE_U64S is computed from size_of::<CppPublisher>() — always exact.

/// Create a publisher on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `CPP_PUBLISHER_OPAQUE_U64S * 8` bytes, aligned to 8 bytes.
/// The publisher is written directly into this buffer (no heap allocation).
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// 8-byte-aligned buffer of at least `CPP_PUBLISHER_OPAQUE_U64S * 8` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_create(
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
        .create_publisher(&topic_info, qos_settings)
    {
        Ok(handle) => {
            let mut pub_handle = CppPublisher {
                handle,
                topic_name: [0u8; 256],
                topic_name_len: topic_str.len().min(255),
            };
            pub_handle.topic_name[..pub_handle.topic_name_len]
                .copy_from_slice(&topic_str.as_bytes()[..pub_handle.topic_name_len]);

            // Write directly into caller-provided storage (no heap allocation)
            unsafe {
                core::ptr::write(storage as *mut CppPublisher, pub_handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Publish raw CDR data.
///
/// # Safety
/// `storage` must be a valid publisher storage. `data` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publish_raw(
    storage: *mut c_void,
    data: *const u8,
    len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || data.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let publisher = unsafe { &*(storage as *const CppPublisher) };
    let data_slice = unsafe { core::slice::from_raw_parts(data, len) };

    match publisher.handle.publish_raw(data_slice) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Destroy a publisher (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized publisher storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut CppPublisher);
    }
    NROS_CPP_RET_OK
}

/// Get the topic name of a publisher.
///
/// # Safety
/// `storage` must be a valid publisher storage, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_get_topic_name(
    storage: *const c_void,
) -> *const c_char {
    if storage.is_null() {
        return core::ptr::null();
    }
    let publisher = unsafe { &*(storage as *const CppPublisher) };
    publisher.topic_name.as_ptr() as *const c_char
}
