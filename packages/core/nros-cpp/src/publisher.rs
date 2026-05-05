//! Publisher FFI functions for the C++ API.
//!
//! Phase 87.6 (thin-wrapper refactor): the caller's opaque storage now holds
//! a bare `RmwPublisher` handle — no more `CppPublisher` wrapper bundling
//! topic-name metadata. The `nros::Publisher<M>` C++ class owns the topic
//! name buffer alongside the storage.

use core::ffi::{c_char, c_void};

use nros_rmw::{Publisher as PublisherTrait, Session, TopicInfo};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

/// Create a publisher on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<RmwPublisher>()` bytes (exposed via `NROS_PUBLISHER_SIZE` in
/// the generated header), aligned to its alignment requirement. The
/// `RmwPublisher` handle is written directly into this buffer.
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// appropriately-aligned buffer of at least `NROS_PUBLISHER_SIZE` bytes.
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
            // Write the bare RmwPublisher handle into caller-provided storage.
            unsafe {
                core::ptr::write(storage as *mut nros::internals::RmwPublisher, handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Publish raw CDR data.
///
/// # Safety
/// `storage` must be a valid publisher storage (initialised by
/// `nros_cpp_publisher_create`). `data` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publish_raw(
    storage: *mut c_void,
    data: *const u8,
    len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || data.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let publisher = unsafe { &*(storage as *const nros::internals::RmwPublisher) };
    let data_slice = unsafe { core::slice::from_raw_parts(data, len) };

    match publisher.publish_raw(data_slice) {
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
        core::ptr::drop_in_place(storage as *mut nros::internals::RmwPublisher);
    }
    NROS_CPP_RET_OK
}

/// Relocate an `RmwPublisher` from `old_storage` to `new_storage`.
///
/// `RmwPublisher` registers nothing externally that references its storage
/// address, so relocation is a straight `ptr::read` + `ptr::write`. Called
/// by the C++ `Publisher` move ctor / move assignment.
///
/// # Safety
/// Both `old_storage` and `new_storage` must be valid, appropriately-aligned
/// buffers of at least `NROS_PUBLISHER_SIZE` bytes. `old_storage` must
/// contain an initialised `RmwPublisher`; `new_storage` must not. After the
/// call, `old_storage` is logically uninitialised and must not be destroyed
/// — the C++ side sets its `initialized_` flag to `false`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut nros::internals::RmwPublisher);
        core::ptr::write(new_storage as *mut nros::internals::RmwPublisher, value);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Phase 108 — publisher-side status events (stub: NROS_CPP_RET_UNSUPPORTED)
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_pub_count_status_t {
    pub total_count: u32,
    pub total_count_change: u32,
}

pub type nros_cpp_publisher_count_cb_t = Option<
    unsafe extern "C" fn(
        storage: *mut c_void,
        status: nros_cpp_pub_count_status_t,
        user_context: *mut c_void,
    ),
>;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_set_liveliness_lost(
    _storage: *mut c_void,
    _cb: nros_cpp_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> nros_cpp_ret_t {
    crate::NROS_CPP_RET_UNSUPPORTED
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_set_offered_deadline_missed(
    _storage: *mut c_void,
    _deadline_ms: u32,
    _cb: nros_cpp_publisher_count_cb_t,
    _user_context: *mut c_void,
) -> nros_cpp_ret_t {
    crate::NROS_CPP_RET_UNSUPPORTED
}

/// Phase 108.B.7 — manually assert this publisher's liveliness.
///
/// Required for entities created with QoS `liveliness_kind =
/// MANUAL_BY_TOPIC` / `MANUAL_BY_NODE`. No-op otherwise. Backends
/// without manual-assertion wiring return `OK` (the trait default).
///
/// # Safety
/// `storage` must be a valid publisher storage (initialised by
/// `nros_cpp_publisher_create`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_publisher_assert_liveliness(
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let publisher = unsafe { &*(storage as *const nros::internals::RmwPublisher) };
    match publisher.assert_liveliness() {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}
