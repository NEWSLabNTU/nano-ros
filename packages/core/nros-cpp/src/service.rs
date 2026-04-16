//! Service server and client FFI functions for the C++ API.

use core::ffi::{c_char, c_void};

use nros_rmw::{ServiceClientTrait, ServiceInfo, ServiceServerTrait, Session};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TIMEOUT, NROS_CPP_RET_TRANSPORT_ERROR, NROS_CPP_RET_TRY_AGAIN, cstr_to_str,
    nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

/// Default receive buffer size for service requests/replies.
const SERVICE_BUF_SIZE: usize = 1024;

// ============================================================================
// Service Server
// ============================================================================

/// Service server wrapper stored in caller-provided inline storage.
pub(crate) struct CppServiceServer {
    handle: nros::internals::RmwServiceServer,
    buffer: [u8; SERVICE_BUF_SIZE],
    service_name: [u8; 256],
    _service_name_len: usize,
}

// CPP_SERVICE_SERVER_OPAQUE_U64S is computed from size_of::<CppServiceServer>() — always exact.

/// Create a service server on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `CPP_SERVICE_SERVER_OPAQUE_U64S * 8` bytes, aligned to 8 bytes.
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// 8-byte-aligned buffer of at least `CPP_SERVICE_SERVER_OPAQUE_U64S * 8` bytes.
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

    // Extract node name/namespace from the node handle
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
            let mut server = CppServiceServer {
                handle,
                buffer: [0u8; SERVICE_BUF_SIZE],
                service_name: [0u8; 256],
                _service_name_len: svc_str.len().min(255),
            };
            server.service_name[..server._service_name_len]
                .copy_from_slice(&svc_str.as_bytes()[..server._service_name_len]);

            // Write directly into caller-provided storage (no heap allocation)
            unsafe {
                core::ptr::write(storage as *mut CppServiceServer, server);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive a raw service request (non-blocking).
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

    let server = unsafe { &mut *(storage as *mut CppServiceServer) };

    match server.handle.try_recv_request(&mut server.buffer) {
        Ok(Some(request)) => {
            let data_len = request.data.len();
            let seq = request.sequence_number;
            // Copy data from buffer to caller's buffer
            if data_len <= out_capacity {
                unsafe {
                    core::ptr::copy_nonoverlapping(server.buffer.as_ptr(), out_data, data_len);
                    *out_len = data_len;
                    *out_sequence = seq;
                }
                NROS_CPP_RET_OK
            } else {
                unsafe {
                    *out_len = data_len;
                    *out_sequence = seq;
                }
                NROS_CPP_RET_ERROR
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

    let server = unsafe { &mut *(storage as *mut CppServiceServer) };
    let data_slice = unsafe { core::slice::from_raw_parts(data, len) };

    match server.handle.send_reply(sequence_number, data_slice) {
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
        core::ptr::drop_in_place(storage as *mut CppServiceServer);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Service Client
// ============================================================================

/// Service client wrapper stored in caller-provided inline storage.
pub(crate) struct CppServiceClient {
    handle: nros::internals::RmwServiceClient,
    buffer: [u8; SERVICE_BUF_SIZE],
    service_name: [u8; 256],
    _service_name_len: usize,
}

// CPP_SERVICE_CLIENT_OPAQUE_U64S is computed from size_of::<CppServiceClient>() — always exact.

/// Create a service client on a node.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `CPP_SERVICE_CLIENT_OPAQUE_U64S * 8` bytes, aligned to 8 bytes.
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
            let mut client = CppServiceClient {
                handle,
                buffer: [0u8; SERVICE_BUF_SIZE],
                service_name: [0u8; 256],
                _service_name_len: svc_str.len().min(255),
            };
            client.service_name[..client._service_name_len]
                .copy_from_slice(&svc_str.as_bytes()[..client._service_name_len]);

            // Write directly into caller-provided storage (no heap allocation)
            unsafe {
                core::ptr::write(storage as *mut CppServiceClient, client);
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

    let client = unsafe { &mut *(storage as *mut CppServiceClient) };
    let req_slice = unsafe { core::slice::from_raw_parts(req_data, req_len) };

    // Use the client's internal buffer for the raw reply
    #[allow(deprecated)]
    match client.handle.call_raw(req_slice, &mut client.buffer) {
        Ok(len) => {
            if len <= resp_capacity {
                unsafe {
                    core::ptr::copy_nonoverlapping(client.buffer.as_ptr(), resp_data, len);
                    *resp_len = len;
                }
                NROS_CPP_RET_OK
            } else {
                unsafe {
                    *resp_len = len;
                }
                NROS_CPP_RET_ERROR
            }
        }
        Err(_) => NROS_CPP_RET_TIMEOUT,
    }
}

/// Send a service request asynchronously (non-blocking).
///
/// The caller must subsequently poll [`nros_cpp_service_client_try_recv_reply`]
/// to receive the response.
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
    let client = unsafe { &mut *(storage as *mut CppServiceClient) };
    let req_slice = unsafe { core::slice::from_raw_parts(req_data, req_len) };
    match client.handle.send_request_raw(req_slice) {
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
    let client = unsafe { &mut *(storage as *mut CppServiceClient) };
    match client.handle.try_recv_reply_raw(&mut client.buffer) {
        Ok(Some(len)) => {
            if len <= resp_capacity {
                unsafe {
                    core::ptr::copy_nonoverlapping(client.buffer.as_ptr(), resp_data, len);
                    *resp_len = len;
                }
                NROS_CPP_RET_OK
            } else {
                unsafe {
                    *resp_len = len;
                }
                NROS_CPP_RET_ERROR
            }
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
        core::ptr::drop_in_place(storage as *mut CppServiceClient);
    }
    NROS_CPP_RET_OK
}
