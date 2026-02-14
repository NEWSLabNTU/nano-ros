//! Service API for nros C API.
//!
//! Services provide request-reply communication patterns.
//! This module implements both service servers and clients.

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

use crate::constants::{MAX_SERVICE_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};
use crate::publisher::nano_ros_message_type_t;
use crate::support::nano_ros_support_state_t;

// ============================================================================
// Service Server
// ============================================================================

/// Service server callback function type.
///
/// # Parameters
/// * `request_data` - Pointer to CDR-serialized request data
/// * `request_len` - Length of request data in bytes
/// * `response_data` - Pointer to buffer for CDR-serialized response
/// * `response_capacity` - Capacity of response buffer
/// * `response_len` - Output: actual length of response data written
/// * `context` - User-provided context pointer
///
/// # Returns
/// * `true` if the request was handled successfully
/// * `false` if there was an error handling the request
pub type nano_ros_service_callback_t = Option<
    unsafe extern "C" fn(
        request_data: *const u8,
        request_len: usize,
        response_data: *mut u8,
        response_capacity: usize,
        response_len: *mut usize,
        context: *mut c_void,
    ) -> bool,
>;

/// Service server state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_service_state_t {
    /// Not initialized
    NANO_ROS_SERVICE_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_SERVICE_STATE_INITIALIZED = 1,
    /// Shutdown
    NANO_ROS_SERVICE_STATE_SHUTDOWN = 2,
}

/// Service server structure.
#[repr(C)]
pub struct nano_ros_service_t {
    /// Current state
    pub state: nano_ros_service_state_t,
    /// Service name storage
    service_name: [u8; MAX_SERVICE_NAME_LEN],
    /// Service name length
    service_name_len: usize,
    /// Type name storage
    type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    type_name_len: usize,
    /// Type hash storage
    type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    type_hash_len: usize,
    /// User callback function
    callback: nano_ros_service_callback_t,
    /// User context pointer
    context: *mut c_void,
    /// Pointer to parent node
    node: *const nros_node_t,
    /// Opaque pointer to internal Rust service server
    _internal: *mut c_void,
}

impl Default for nano_ros_service_t {
    fn default() -> Self {
        Self {
            state: nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_UNINITIALIZED,
            service_name: [0u8; MAX_SERVICE_NAME_LEN],
            service_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ptr::null_mut(),
        }
    }
}

impl nano_ros_service_t {
    /// Get the callback function
    pub(crate) fn get_callback(&self) -> nano_ros_service_callback_t {
        self.callback
    }

    /// Get the context pointer
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }

    /// Get the internal handle pointer
    pub(crate) fn get_internal(&self) -> *mut c_void {
        self._internal
    }
}

/// Get a zero-initialized service server.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_service_get_zero_initialized() -> nano_ros_service_t {
    nano_ros_service_t::default()
}

/// Initialize a service server.
///
/// # Parameters
/// * `service` - Pointer to a zero-initialized service
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to service type information
/// * `service_name` - Service name (null-terminated string)
/// * `callback` - Callback function to invoke when requests arrive
/// * `context` - User context pointer passed to callback (can be NULL)
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if node is not initialized
/// * `NANO_ROS_RET_ERROR` on initialization failure
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_service_init(
    service: *mut nano_ros_service_t,
    node: *const nros_node_t,
    type_info: *const nano_ros_message_type_t,
    service_name: *const c_char,
    callback: nano_ros_service_callback_t,
    context: *mut c_void,
) -> nano_ros_ret_t {
    // Validate required arguments
    if service.is_null() || node.is_null() || type_info.is_null() || service_name.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    if callback.is_none() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let service = &mut *service;
    let node_ref = &*node;
    let type_info = &*type_info;

    // Check if service is already initialized
    if service.state != nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_UNINITIALIZED {
        return NANO_ROS_RET_BAD_SEQUENCE;
    }

    // Check if node is initialized
    if node_ref.state != nros_node_state_t::NANO_ROS_NODE_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Copy service name
    let name_ptr = service_name as *const u8;
    let mut len = 0usize;
    while len < MAX_SERVICE_NAME_LEN - 1 {
        let c = *name_ptr.add(len);
        if c == 0 {
            break;
        }
        service.service_name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }
    service.service_name[len] = 0;
    service.service_name_len = len;

    // Copy type name
    if !type_info.type_name.is_null() {
        let type_ptr = type_info.type_name as *const u8;
        len = 0;
        while len < MAX_TYPE_NAME_LEN - 1 {
            let c = *type_ptr.add(len);
            if c == 0 {
                break;
            }
            service.type_name[len] = c;
            len += 1;
        }
        service.type_name[len] = 0;
        service.type_name_len = len;
    }

    // Copy type hash
    if !type_info.type_hash.is_null() {
        let hash_ptr = type_info.type_hash as *const u8;
        len = 0;
        while len < MAX_TYPE_HASH_LEN - 1 {
            let c = *hash_ptr.add(len);
            if c == 0 {
                break;
            }
            service.type_hash[len] = c;
            len += 1;
        }
        service.type_hash[len] = 0;
        service.type_hash_len = len;
    }

    // Store callback and context
    service.callback = callback;
    service.context = context;
    service.node = node;

    // Create the internal service server using zenoh
    #[cfg(feature = "alloc")]
    {
        use nros_rmw::{ServiceInfo, Session};
        use nros_rmw_zenoh::ShimSession;

        // Get mutable support reference to access the session
        let support_mut = match node_ref.get_support_mut() {
            Some(s) => s,
            None => return NANO_ROS_RET_NOT_INIT,
        };

        if support_mut.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
            return NANO_ROS_RET_NOT_INIT;
        }

        // Save domain_id before borrowing session
        let domain_id = support_mut.domain_id as u32;

        // Get mutable session reference
        let session: &mut ShimSession = match support_mut.get_session_mut() {
            Some(s) => s,
            None => return NANO_ROS_RET_NOT_INIT,
        };

        // Build ServiceInfo
        let svc_name_str =
            core::str::from_utf8_unchecked(&service.service_name[..service.service_name_len]);
        let type_str = core::str::from_utf8_unchecked(&service.type_name[..service.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&service.type_hash[..service.type_hash_len]);

        let svc_info =
            ServiceInfo::new(svc_name_str, type_str, type_hash_str).with_domain(domain_id);

        // Create service server
        match session.create_service_server(&svc_info) {
            Ok(server_handle) => {
                let server_box = alloc::boxed::Box::new(server_handle);
                service._internal = alloc::boxed::Box::into_raw(server_box) as *mut _;
            }
            Err(_) => return NANO_ROS_RET_ERROR,
        }

        service.state = nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED;
        NANO_ROS_RET_OK
    }

    #[cfg(not(feature = "alloc"))]
    {
        // For no_std, not yet implemented
        NANO_ROS_RET_ERROR
    }
}

/// Finalize a service server.
///
/// # Parameters
/// * `service` - Pointer to an initialized service
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if service is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_service_fini(service: *mut nano_ros_service_t) -> nano_ros_ret_t {
    if service.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let service = &mut *service;

    if service.state != nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Clean up internal resources
    #[cfg(feature = "alloc")]
    {
        if !service._internal.is_null() {
            use nros_rmw_zenoh::ShimServiceServer;
            let _server = alloc::boxed::Box::from_raw(service._internal as *mut ShimServiceServer);
            // Server is dropped here
        }
    }

    service._internal = ptr::null_mut();
    service.callback = None;
    service.context = ptr::null_mut();
    service.node = ptr::null();
    service.state = nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

/// Take a service request (non-blocking).
///
/// # Parameters
/// * `service` - Pointer to an initialized service
/// * `request_data` - Buffer to receive CDR-serialized request data
/// * `request_capacity` - Capacity of request buffer
/// * `request_len` - Output: actual length of request data
/// * `sequence_number` - Output: sequence number for response matching
///
/// # Returns
/// * `NANO_ROS_RET_OK` if a request was received
/// * `NANO_ROS_RET_TIMEOUT` if no request is available
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_service_take_request(
    service: *mut nano_ros_service_t,
    request_data: *mut u8,
    request_capacity: usize,
    request_len: *mut usize,
    sequence_number: *mut i64,
) -> nano_ros_ret_t {
    if service.is_null()
        || request_data.is_null()
        || request_len.is_null()
        || sequence_number.is_null()
    {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let service = &mut *service;

    if service.state != nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    #[cfg(feature = "alloc")]
    {
        use nros_rmw::ServiceServerTrait;
        use nros_rmw_zenoh::ShimServiceServer;

        if service._internal.is_null() {
            return NANO_ROS_RET_NOT_INIT;
        }

        let server = &mut *(service._internal as *mut ShimServiceServer);

        // Create a temporary buffer using the provided buffer
        let buf = core::slice::from_raw_parts_mut(request_data, request_capacity);

        match server.try_recv_request(buf) {
            Ok(Some(req)) => {
                *request_len = req.data.len();
                *sequence_number = req.sequence_number;
                NANO_ROS_RET_OK
            }
            Ok(None) => NANO_ROS_RET_TIMEOUT,
            Err(_) => NANO_ROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    {
        NANO_ROS_RET_ERROR
    }
}

/// Send a service response.
///
/// # Parameters
/// * `service` - Pointer to an initialized service
/// * `sequence_number` - Sequence number from the request
/// * `response_data` - CDR-serialized response data
/// * `response_len` - Length of response data
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
/// * `NANO_ROS_RET_ERROR` on send failure
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_service_send_response(
    service: *mut nano_ros_service_t,
    sequence_number: i64,
    response_data: *const u8,
    response_len: usize,
) -> nano_ros_ret_t {
    if service.is_null() || response_data.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let service = &mut *service;

    if service.state != nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    #[cfg(feature = "alloc")]
    {
        use nros_rmw::ServiceServerTrait;
        use nros_rmw_zenoh::ShimServiceServer;

        if service._internal.is_null() {
            return NANO_ROS_RET_NOT_INIT;
        }

        let server = &mut *(service._internal as *mut ShimServiceServer);
        let data = core::slice::from_raw_parts(response_data, response_len);

        match server.send_reply(sequence_number, data) {
            Ok(()) => NANO_ROS_RET_OK,
            Err(_) => NANO_ROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    {
        NANO_ROS_RET_ERROR
    }
}

/// Get the service name.
///
/// # Parameters
/// * `service` - Pointer to a service
///
/// # Returns
/// * Pointer to service name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_service_get_service_name(
    service: *const nano_ros_service_t,
) -> *const c_char {
    if service.is_null() {
        return ptr::null();
    }

    let service = &*service;
    if service.state != nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED {
        return ptr::null();
    }

    service.service_name.as_ptr() as *const c_char
}

/// Check if service is valid (initialized).
///
/// # Parameters
/// * `service` - Pointer to a service
///
/// # Returns
/// * Non-zero if valid, 0 if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_service_is_valid(service: *const nano_ros_service_t) -> c_int {
    if service.is_null() {
        return 0;
    }

    let service = &*service;
    if service.state == nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED {
        1
    } else {
        0
    }
}

// ============================================================================
// Service Client
// ============================================================================

/// Client state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_client_state_t {
    /// Not initialized
    NANO_ROS_CLIENT_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_CLIENT_STATE_INITIALIZED = 1,
    /// Shutdown
    NANO_ROS_CLIENT_STATE_SHUTDOWN = 2,
}

/// Service client structure.
#[repr(C)]
pub struct nano_ros_client_t {
    /// Current state
    pub state: nano_ros_client_state_t,
    /// Service name storage
    service_name: [u8; MAX_SERVICE_NAME_LEN],
    /// Service name length
    service_name_len: usize,
    /// Type name storage
    type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    type_name_len: usize,
    /// Type hash storage
    type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    type_hash_len: usize,
    /// Pointer to parent node
    node: *const nros_node_t,
    /// Opaque pointer to internal Rust service client
    _internal: *mut c_void,
}

impl Default for nano_ros_client_t {
    fn default() -> Self {
        Self {
            state: nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_UNINITIALIZED,
            service_name: [0u8; MAX_SERVICE_NAME_LEN],
            service_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            node: ptr::null(),
            _internal: ptr::null_mut(),
        }
    }
}

/// Get a zero-initialized client.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_client_get_zero_initialized() -> nano_ros_client_t {
    nano_ros_client_t::default()
}

/// Initialize a service client.
///
/// # Parameters
/// * `client` - Pointer to a zero-initialized client
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to service type information
/// * `service_name` - Service name (null-terminated string)
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if node is not initialized
/// * `NANO_ROS_RET_ERROR` on initialization failure
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_client_init(
    client: *mut nano_ros_client_t,
    node: *const nros_node_t,
    type_info: *const nano_ros_message_type_t,
    service_name: *const c_char,
) -> nano_ros_ret_t {
    // Validate required arguments
    if client.is_null() || node.is_null() || type_info.is_null() || service_name.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;
    let node_ref = &*node;
    let type_info = &*type_info;

    // Check if client is already initialized
    if client.state != nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_UNINITIALIZED {
        return NANO_ROS_RET_BAD_SEQUENCE;
    }

    // Check if node is initialized
    if node_ref.state != nros_node_state_t::NANO_ROS_NODE_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Copy service name
    let name_ptr = service_name as *const u8;
    let mut len = 0usize;
    while len < MAX_SERVICE_NAME_LEN - 1 {
        let c = *name_ptr.add(len);
        if c == 0 {
            break;
        }
        client.service_name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }
    client.service_name[len] = 0;
    client.service_name_len = len;

    // Copy type name
    if !type_info.type_name.is_null() {
        let type_ptr = type_info.type_name as *const u8;
        len = 0;
        while len < MAX_TYPE_NAME_LEN - 1 {
            let c = *type_ptr.add(len);
            if c == 0 {
                break;
            }
            client.type_name[len] = c;
            len += 1;
        }
        client.type_name[len] = 0;
        client.type_name_len = len;
    }

    // Copy type hash
    if !type_info.type_hash.is_null() {
        let hash_ptr = type_info.type_hash as *const u8;
        len = 0;
        while len < MAX_TYPE_HASH_LEN - 1 {
            let c = *hash_ptr.add(len);
            if c == 0 {
                break;
            }
            client.type_hash[len] = c;
            len += 1;
        }
        client.type_hash[len] = 0;
        client.type_hash_len = len;
    }

    // Store node pointer
    client.node = node;

    // Create the internal service client using zenoh
    #[cfg(feature = "alloc")]
    {
        use nros_rmw::{ServiceInfo, Session};
        use nros_rmw_zenoh::ShimSession;

        // Get mutable support reference to access the session
        let support_mut = match node_ref.get_support_mut() {
            Some(s) => s,
            None => return NANO_ROS_RET_NOT_INIT,
        };

        if support_mut.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
            return NANO_ROS_RET_NOT_INIT;
        }

        // Save domain_id before borrowing session
        let domain_id = support_mut.domain_id as u32;

        // Get mutable session reference
        let session: &mut ShimSession = match support_mut.get_session_mut() {
            Some(s) => s,
            None => return NANO_ROS_RET_NOT_INIT,
        };

        // Build ServiceInfo
        let svc_name_str =
            core::str::from_utf8_unchecked(&client.service_name[..client.service_name_len]);
        let type_str = core::str::from_utf8_unchecked(&client.type_name[..client.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&client.type_hash[..client.type_hash_len]);

        let svc_info =
            ServiceInfo::new(svc_name_str, type_str, type_hash_str).with_domain(domain_id);

        // Create service client
        match session.create_service_client(&svc_info) {
            Ok(client_handle) => {
                let client_box = alloc::boxed::Box::new(client_handle);
                client._internal = alloc::boxed::Box::into_raw(client_box) as *mut _;
            }
            Err(_) => return NANO_ROS_RET_ERROR,
        }

        client.state = nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_INITIALIZED;
        NANO_ROS_RET_OK
    }

    #[cfg(not(feature = "alloc"))]
    {
        // For no_std, not yet implemented
        NANO_ROS_RET_ERROR
    }
}

/// Finalize a service client.
///
/// # Parameters
/// * `client` - Pointer to an initialized client
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if client is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_client_fini(client: *mut nano_ros_client_t) -> nano_ros_ret_t {
    if client.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;

    if client.state != nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Clean up internal resources
    #[cfg(feature = "alloc")]
    {
        if !client._internal.is_null() {
            use nros_rmw_zenoh::ShimServiceClient;
            let _client_handle =
                alloc::boxed::Box::from_raw(client._internal as *mut ShimServiceClient);
            // Client is dropped here
        }
    }

    client._internal = ptr::null_mut();
    client.node = ptr::null();
    client.state = nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

/// Call a service (blocking).
///
/// This function sends a request and blocks until a response is received
/// or a timeout occurs.
///
/// # Parameters
/// * `client` - Pointer to an initialized client
/// * `request_data` - CDR-serialized request data
/// * `request_len` - Length of request data
/// * `response_data` - Buffer to receive CDR-serialized response
/// * `response_capacity` - Capacity of response buffer
/// * `response_len` - Output: actual length of response data
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
/// * `NANO_ROS_RET_TIMEOUT` if no response within timeout
/// * `NANO_ROS_RET_ERROR` on call failure
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_client_call(
    client: *mut nano_ros_client_t,
    request_data: *const u8,
    request_len: usize,
    response_data: *mut u8,
    response_capacity: usize,
    response_len: *mut usize,
) -> nano_ros_ret_t {
    if client.is_null()
        || request_data.is_null()
        || response_data.is_null()
        || response_len.is_null()
    {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;

    if client.state != nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    #[cfg(feature = "alloc")]
    {
        use nros_rmw::ServiceClientTrait;
        use nros_rmw_zenoh::ShimServiceClient;

        if client._internal.is_null() {
            return NANO_ROS_RET_NOT_INIT;
        }

        let client_handle = &mut *(client._internal as *mut ShimServiceClient);
        let request = core::slice::from_raw_parts(request_data, request_len);
        let reply_buf = core::slice::from_raw_parts_mut(response_data, response_capacity);

        match client_handle.call_raw(request, reply_buf) {
            Ok(len) => {
                *response_len = len;
                NANO_ROS_RET_OK
            }
            Err(nros_rmw::TransportError::Timeout) => NANO_ROS_RET_TIMEOUT,
            Err(_) => NANO_ROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    {
        NANO_ROS_RET_ERROR
    }
}

/// Get the service name of a client.
///
/// # Parameters
/// * `client` - Pointer to a client
///
/// # Returns
/// * Pointer to service name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_client_get_service_name(
    client: *const nano_ros_client_t,
) -> *const c_char {
    if client.is_null() {
        return ptr::null();
    }

    let client = &*client;
    if client.state != nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_INITIALIZED {
        return ptr::null();
    }

    client.service_name.as_ptr() as *const c_char
}

/// Check if client is valid (initialized).
///
/// # Parameters
/// * `client` - Pointer to a client
///
/// # Returns
/// * Non-zero if valid, 0 if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_client_is_valid(client: *const nano_ros_client_t) -> c_int {
    if client.is_null() {
        return 0;
    }

    let client = &*client;
    if client.state == nano_ros_client_state_t::NANO_ROS_CLIENT_STATE_INITIALIZED {
        1
    } else {
        0
    }
}
