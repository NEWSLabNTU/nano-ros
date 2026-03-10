//! Service server and client FFI functions for the C++ API.

use core::ffi::{c_char, c_void};

use nros_rmw::{ServiceClientTrait, ServiceInfo, ServiceServerTrait, Session};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TIMEOUT, NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t,
    nros_cpp_qos_t, nros_cpp_ret_t,
};

/// Default receive buffer size for service requests/replies.
const SERVICE_BUF_SIZE: usize = 1024;

// ============================================================================
// Service Server
// ============================================================================

/// Boxed service server handle stored behind `void*`.
struct CppServiceServer {
    handle: nros::internals::RmwServiceServer,
    buffer: [u8; SERVICE_BUF_SIZE],
    service_name: [u8; 256],
    _service_name_len: usize,
}

/// Create a service server on a node.
///
/// # Parameters
/// * `node` — Node handle from `nros_cpp_node_create()`.
/// * `service_name` — Service name (null-terminated).
/// * `type_name` — ROS service type name (null-terminated).
/// * `type_hash` — ROS type hash string (null-terminated).
/// * `qos` — QoS settings (currently unused for services, reserved).
/// * `out_handle` — Receives the opaque service server handle on success.
///
/// # Safety
/// All pointer parameters must be valid. `out_handle` must point to a `*mut c_void`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_create(
    node: *const nros_cpp_node_t,
    service_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    _qos: nros_cpp_qos_t,
    out_handle: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || service_name.is_null()
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

            let boxed = alloc::boxed::Box::new(server);
            unsafe {
                *out_handle = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive a raw service request (non-blocking).
///
/// # Parameters
/// * `handle` — Service server handle.
/// * `out_data` — Caller's buffer to receive CDR request data.
/// * `out_capacity` — Size of the caller's buffer.
/// * `out_len` — Receives the number of bytes written (0 if no request).
/// * `out_sequence` — Receives the request sequence number for reply matching.
///
/// # Returns
/// * `NROS_CPP_RET_OK` — Request received or no request pending (`*out_len == 0`).
/// * `NROS_CPP_RET_ERROR` — Transport error.
///
/// # Safety
/// All pointers must be valid. `out_data` must point to `out_capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_try_recv_raw(
    handle: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
    out_sequence: *mut i64,
) -> nros_cpp_ret_t {
    if handle.is_null() || out_data.is_null() || out_len.is_null() || out_sequence.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &mut *(handle as *mut CppServiceServer) };

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
/// # Parameters
/// * `handle` — Service server handle.
/// * `sequence_number` — Sequence number from the received request.
/// * `data` — CDR-serialized reply data.
/// * `len` — Length of reply data.
///
/// # Safety
/// `handle` must be valid. `data` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_send_reply_raw(
    handle: *mut c_void,
    sequence_number: i64,
    data: *const u8,
    len: usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || data.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &mut *(handle as *mut CppServiceServer) };
    let data_slice = unsafe { core::slice::from_raw_parts(data, len) };

    match server.handle.send_reply(sequence_number, data_slice) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Destroy a service server and free its resources.
///
/// # Safety
/// `handle` must be a valid service server handle, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_destroy(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        let _server = alloc::boxed::Box::from_raw(handle as *mut CppServiceServer);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Service Client
// ============================================================================

/// Boxed service client handle stored behind `void*`.
struct CppServiceClient {
    handle: nros::internals::RmwServiceClient,
    buffer: [u8; SERVICE_BUF_SIZE],
    service_name: [u8; 256],
    _service_name_len: usize,
}

/// Create a service client on a node.
///
/// # Parameters
/// * `node` — Node handle from `nros_cpp_node_create()`.
/// * `service_name` — Service name (null-terminated).
/// * `type_name` — ROS service type name (null-terminated).
/// * `type_hash` — ROS type hash string (null-terminated).
/// * `qos` — QoS settings (currently unused for services, reserved).
/// * `out_handle` — Receives the opaque service client handle on success.
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
    out_handle: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || service_name.is_null()
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

            let boxed = alloc::boxed::Box::new(client);
            unsafe {
                *out_handle = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Send a service request and block for reply (raw CDR).
///
/// # Parameters
/// * `handle` — Service client handle.
/// * `req_data` — CDR-serialized request.
/// * `req_len` — Request length.
/// * `resp_data` — Buffer for CDR-serialized reply.
/// * `resp_capacity` — Reply buffer capacity.
/// * `resp_len` — Receives actual reply length.
///
/// # Returns
/// * `NROS_CPP_RET_OK` on success.
/// * `NROS_CPP_RET_TIMEOUT` on timeout.
/// * `NROS_CPP_RET_ERROR` on error.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_call_raw(
    handle: *mut c_void,
    req_data: *const u8,
    req_len: usize,
    resp_data: *mut u8,
    resp_capacity: usize,
    resp_len: *mut usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || req_data.is_null() || resp_data.is_null() || resp_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppServiceClient) };
    let req_slice = unsafe { core::slice::from_raw_parts(req_data, req_len) };

    // Use the client's internal buffer for the raw reply
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

/// Destroy a service client and free its resources.
///
/// # Safety
/// `handle` must be a valid service client handle, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_destroy(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        let _client = alloc::boxed::Box::from_raw(handle as *mut CppServiceClient);
    }
    NROS_CPP_RET_OK
}
