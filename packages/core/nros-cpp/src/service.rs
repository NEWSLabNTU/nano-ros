//! Service server and client FFI functions for the C++ API.
//!
//! Phase 87.6 (thin-wrapper refactor): caller's opaque storage holds a bare
//! `RmwServiceServer` / `RmwServiceClient` handle. Service-name buffers live
//! on the C++ `nros::Service<S>` / `nros::Client<S>` classes. Received CDR
//! bytes are copied directly into caller-provided output buffers — no
//! Rust-side scratch.

use core::ffi::{c_char, c_void};

use nros_rmw::{ServiceClientTrait, ServiceInfo, ServiceServerTrait, Session};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TIMEOUT, NROS_CPP_RET_TRANSPORT_ERROR, NROS_CPP_RET_TRY_AGAIN, cstr_to_str,
    nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

// ============================================================================
// Service Server
// ============================================================================

/// Create a service server on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<RmwServiceServer>()` bytes (exposed via `NROS_SERVICE_SERVER_SIZE`).
///
/// # Safety
/// All pointer parameters must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_create(
    node: *const nros_cpp_node_t,
    service_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    _qos: nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || service_name.is_null()
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

    let svc_str = match unsafe { cstr_to_str(service_name) } {
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

    let mut svc_info = ServiceInfo::new(svc_str, type_str, hash_str)
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str);
    if let Some(name) = node_name_str
        && !name.is_empty()
    {
        svc_info = svc_info.with_node_name(name);
    }

    match ctx.executor.session_mut().create_service_server(&svc_info) {
        Ok(handle) => {
            unsafe {
                core::ptr::write(storage as *mut nros::internals::RmwServiceServer, handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive a raw service request (non-blocking).
///
/// Writes the received CDR bytes directly into the caller's output buffer —
/// no Rust-side scratch.
///
/// # Safety
/// All pointers must be valid. `out_data` must point to `out_capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_try_recv_raw(
    storage: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
    out_sequence: *mut i64,
) -> nros_cpp_ret_t {
    if storage.is_null() || out_data.is_null() || out_len.is_null() || out_sequence.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &mut *(storage as *mut nros::internals::RmwServiceServer) };
    let out_slice = unsafe { core::slice::from_raw_parts_mut(out_data, out_capacity) };

    match server.try_recv_request(out_slice) {
        Ok(Some(request)) => {
            unsafe {
                *out_len = request.data.len();
                *out_sequence = request.sequence_number;
            }
            NROS_CPP_RET_OK
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

/// Send a raw reply to a service request.
///
/// # Safety
/// `storage` must be valid. `data` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_send_reply_raw(
    storage: *mut c_void,
    sequence_number: i64,
    data: *const u8,
    len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || data.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &mut *(storage as *mut nros::internals::RmwServiceServer) };
    let data_slice = unsafe { core::slice::from_raw_parts(data, len) };

    match server.send_reply(sequence_number, data_slice) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Destroy a service server (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized service server storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut nros::internals::RmwServiceServer);
    }
    NROS_CPP_RET_OK
}

/// Relocate an `RmwServiceServer` from `old_storage` to `new_storage`.
///
/// # Safety
/// See `nros_cpp_publisher_relocate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut nros::internals::RmwServiceServer);
        core::ptr::write(new_storage as *mut nros::internals::RmwServiceServer, value);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Service Client
// ============================================================================

/// Create a service client on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<RmwServiceClient>()` bytes (exposed via `NROS_SERVICE_CLIENT_SIZE`).
///
/// # Safety
/// All pointer parameters must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_create(
    node: *const nros_cpp_node_t,
    service_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    _qos: nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || service_name.is_null()
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

    let svc_str = match unsafe { cstr_to_str(service_name) } {
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

    let mut svc_info = ServiceInfo::new(svc_str, type_str, hash_str)
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str);
    if let Some(name) = node_name_str
        && !name.is_empty()
    {
        svc_info = svc_info.with_node_name(name);
    }

    match ctx.executor.session_mut().create_service_client(&svc_info) {
        Ok(handle) => {
            unsafe {
                core::ptr::write(storage as *mut nros::internals::RmwServiceClient, handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Send a service request and block for reply (raw CDR).
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_call_raw(
    storage: *mut c_void,
    req_data: *const u8,
    req_len: usize,
    resp_data: *mut u8,
    resp_capacity: usize,
    resp_len: *mut usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || req_data.is_null() || resp_data.is_null() || resp_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(storage as *mut nros::internals::RmwServiceClient) };
    let req_slice = unsafe { core::slice::from_raw_parts(req_data, req_len) };
    let resp_slice = unsafe { core::slice::from_raw_parts_mut(resp_data, resp_capacity) };

    // Deprecated blocking call via zpico_get. Prefer send_request +
    // try_recv_reply on platforms without threads.
    #[allow(deprecated)]
    match client.call_raw(req_slice, resp_slice) {
        Ok(len) => {
            unsafe {
                *resp_len = len;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TIMEOUT,
    }
}

/// Send a service request asynchronously (non-blocking).
///
/// # Safety
/// `storage` must be a valid initialized service client. `req_data` must point
/// to `req_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_send_request(
    storage: *mut c_void,
    req_data: *const u8,
    req_len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || req_data.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(storage as *mut nros::internals::RmwServiceClient) };
    let req_slice = unsafe { core::slice::from_raw_parts(req_data, req_len) };
    match client.send_request_raw(req_slice) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive a reply (non-blocking).
///
/// Returns `NROS_CPP_RET_OK` and fills `resp_data`/`resp_len` on success,
/// `NROS_CPP_RET_TRY_AGAIN` if no reply is available yet.
///
/// # Safety
/// All pointers must be valid. `resp_data` must point to `resp_capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_try_recv_reply(
    storage: *mut c_void,
    resp_data: *mut u8,
    resp_capacity: usize,
    resp_len: *mut usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || resp_data.is_null() || resp_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(storage as *mut nros::internals::RmwServiceClient) };
    let resp_slice = unsafe { core::slice::from_raw_parts_mut(resp_data, resp_capacity) };

    match client.try_recv_reply_raw(resp_slice) {
        Ok(Some(len)) => {
            unsafe {
                *resp_len = len;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe {
                *resp_len = 0;
            }
            NROS_CPP_RET_TRY_AGAIN
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Destroy a service client (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized service client storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut nros::internals::RmwServiceClient);
    }
    NROS_CPP_RET_OK
}

/// Relocate an `RmwServiceClient` from `old_storage` to `new_storage`.
///
/// # Safety
/// See `nros_cpp_publisher_relocate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut nros::internals::RmwServiceClient);
        core::ptr::write(new_storage as *mut nros::internals::RmwServiceClient, value);
    }
    NROS_CPP_RET_OK
}
