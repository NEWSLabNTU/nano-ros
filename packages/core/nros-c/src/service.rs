//! Service API for nros C API.
//!
//! Services provide request-reply communication patterns.
//! This module implements both service servers and clients.

use core::ffi::{c_char, c_void};
use core::ptr;
use core::sync::atomic::AtomicBool;
use core::task::{RawWaker, RawWakerVTable, Waker};

use crate::constants::{MAX_SERVICE_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::executor::nros_executor_t;
use crate::node::{nros_node_state_t, nros_node_t};
use crate::publisher::nros_service_type_t;

// ============================================================================
// Waker helper — creates a Waker that sets an AtomicBool
// ============================================================================

/// Create a [`Waker`] that sets the given [`AtomicBool`] to `true` when woken.
///
/// The `AtomicBool` must outlive the waker. Used to bridge the transport's
/// reply notification (`register_waker`) to the arena entry's `reply_ready`
/// flag, avoiding blind polling of `get_check` on every spin tick.
fn atomic_bool_waker(flag: &AtomicBool) -> Waker {
    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        // clone: return a new RawWaker pointing to the same flag
        |data| RawWaker::new(data, &VTABLE),
        // wake: set the flag (by value — consumes the waker)
        |data| unsafe {
            (*(data as *const AtomicBool)).store(true, core::sync::atomic::Ordering::Release);
        },
        // wake_by_ref: set the flag (by reference — waker stays alive)
        |data| unsafe {
            (*(data as *const AtomicBool)).store(true, core::sync::atomic::Ordering::Release);
        },
        // drop: no-op (flag is borrowed, not owned)
        |_data| {},
    );
    let raw = RawWaker::new(flag as *const AtomicBool as *const (), &VTABLE);
    // SAFETY: the vtable is valid and the flag outlives the waker (it lives
    // in the arena entry which outlives any single spin iteration).
    unsafe { Waker::from_raw(raw) }
}

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
pub type nros_service_callback_t = Option<
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
pub enum nros_service_state_t {
    /// Not initialized
    NROS_SERVICE_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_SERVICE_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_SERVICE_STATE_SHUTDOWN = 2,
}

/// Service server structure.
#[repr(C)]
pub struct nros_service_t {
    /// Current state
    pub state: nros_service_state_t,
    /// Service name storage
    pub service_name: [u8; MAX_SERVICE_NAME_LEN],
    /// Service name length
    pub service_name_len: usize,
    /// Type name storage
    pub type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    pub type_name_len: usize,
    /// Type hash storage
    pub type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    pub type_hash_len: usize,
    /// User callback function
    pub callback: nros_service_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Internal state (arena entry index + executor pointer). Phase 87.5:
    /// now a typed `#[repr(C)]` field instead of a `[u64; N]` opaque blob.
    pub _internal: ServiceServerInternal,
}

impl Default for nros_service_t {
    fn default() -> Self {
        Self {
            state: nros_service_state_t::NROS_SERVICE_STATE_UNINITIALIZED,
            service_name: [0u8; MAX_SERVICE_NAME_LEN],
            service_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ServiceServerInternal::new(),
        }
    }
}

impl nros_service_t {
    /// Get the callback function
    pub(crate) fn get_callback(&self) -> nros_service_callback_t {
        self.callback
    }

    /// Get the context pointer
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }
}

/// Get a zero-initialized service server.
#[unsafe(no_mangle)]
pub extern "C" fn nros_service_get_zero_initialized() -> nros_service_t {
    nros_service_t::default()
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
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NROS_RET_NOT_INIT` if node is not initialized
/// * `NROS_RET_ERROR` on initialization failure
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_service_init(
    service: *mut nros_service_t,
    node: *const nros_node_t,
    type_info: *const nros_service_type_t,
    service_name: *const c_char,
    callback: nros_service_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(service, node, type_info, service_name);

    if callback.is_none() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let service = &mut *service;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        service,
        nros_service_state_t::NROS_SERVICE_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy service name (required — empty rejected)
    service.service_name_len =
        crate::util::copy_cstr_into(service_name, &mut service.service_name);
    if service.service_name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    service.type_name_len =
        crate::util::copy_cstr_into(type_info.type_name, &mut service.type_name);
    service.type_hash_len =
        crate::util::copy_cstr_into(type_info.type_hash, &mut service.type_hash);

    // Store callback and context
    service.callback = callback;
    service.context = context;
    service.node = node;

    // Service server creation is deferred to nros_executor_add_service(),
    // which calls nros_node::Executor::add_service_raw_sized().
    // Initialise the internal state (executor_ptr null until registration).
    service._internal = ServiceServerInternal::new();
    service.state = nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Finalize a service server.
///
/// # Parameters
/// * `service` - Pointer to an initialized service
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if service is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_service_fini(service: *mut nros_service_t) -> nros_ret_t {
    validate_not_null!(service);

    let service = &mut *service;

    validate_state!(
        service,
        nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED
    );

    // Reset the inline ServiceServerInternal. The actual service server
    // lives in the executor's arena and is freed when the executor is
    // destroyed; this struct has no Drop impl, so a simple overwrite
    // with `new()` is sufficient.
    service._internal = ServiceServerInternal::new();
    service.callback = None;
    service.context = ptr::null_mut();
    service.node = ptr::null();
    service.state = nros_service_state_t::NROS_SERVICE_STATE_SHUTDOWN;

    NROS_RET_OK
}

/// Take a service request (non-blocking).
///
/// Currently not supported — service servers are callback-only through
/// the executor. Use `nros_executor_add_service()` with a callback instead.
///
/// # Returns
/// * `NROS_RET_NOT_INIT` always (manual poll not supported)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_service_take_request(
    service: *mut nros_service_t,
    _request_data: *mut u8,
    _request_capacity: usize,
    _request_len: *mut usize,
    _sequence_number: *mut i64,
) -> nros_ret_t {
    validate_not_null!(service);
    // Service server handles live in the executor arena — manual poll
    // is not supported. Use executor callbacks instead.
    NROS_RET_NOT_INIT
}

/// Send a service response.
///
/// Currently not supported — service servers are callback-only through
/// the executor. The callback's return value and response buffer are used
/// to send the response automatically.
///
/// # Returns
/// * `NROS_RET_NOT_INIT` always (manual poll not supported)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_service_send_response(
    service: *mut nros_service_t,
    _sequence_number: i64,
    _response_data: *const u8,
    _response_len: usize,
) -> nros_ret_t {
    validate_not_null!(service);
    // Service server handles live in the executor arena — manual send
    // is not supported. Use executor callbacks instead.
    NROS_RET_NOT_INIT
}

/// Get the service name.
///
/// # Parameters
/// * `service` - Pointer to a service
///
/// # Returns
/// * Pointer to service name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_service_get_service_name(
    service: *const nros_service_t,
) -> *const c_char {
    if service.is_null() {
        return ptr::null();
    }

    let service = &*service;
    if service.state != nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED {
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
/// * `true` if valid, `false` if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_service_is_valid(service: *const nros_service_t) -> bool {
    if service.is_null() {
        return false;
    }

    let service = &*service;
    service.state == nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED
}

// ============================================================================
// Service Server Internal
// ============================================================================

/// Internal state for the service server (Phase 82.7).
///
/// Lightweight — stores only the arena entry index and the executor
/// pointer where the actual transport handle lives. Mirrors
/// `ServiceClientInternal` and `ActionClientInternal`.
///
/// Phase 87.5: `#[repr(C)]` gives deterministic layout so cbindgen can
/// emit this struct into the C header. The size is determined by the
/// struct definition directly — no hand-math or `u64s_for::<T>()` probe
/// required.
#[repr(C)]
pub struct ServiceServerInternal {
    /// Arena entry index. -1 means not registered with any executor yet.
    pub arena_entry_index: i32,
    /// Pointer to the outer `nros_executor_t` that owns the arena entry.
    pub executor_ptr: *mut c_void,
}

impl ServiceServerInternal {
    pub const fn new() -> Self {
        Self {
            arena_entry_index: -1,
            executor_ptr: core::ptr::null_mut(),
        }
    }
}

impl Default for ServiceServerInternal {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Service Client
// ============================================================================

/// Default service-client RPC timeout in milliseconds (Phase 82).
///
/// `nros_client_call` reads this from `ServiceClientInternal.timeout_ms`,
/// which is initialised to this value by `nros_client_init` and can be
/// changed at any time via `nros_client_set_timeout`.
const NROS_DEFAULT_SERVICE_TIMEOUT_MS: u32 = 5000;

/// Service-client response callback type (Phase 82).
///
/// Invoked by the executor's `spin_some` dispatch when a previously-sent
/// request has its response delivered. The CDR bytes are owned by the
/// arena entry's reply buffer for the duration of the call — copy if you
/// need to keep them.
pub type nros_response_callback_t =
    Option<unsafe extern "C" fn(response: *const u8, response_len: usize, context: *mut c_void)>;

/// Internal state for the service client (Phase 82).
///
/// Lightweight — stores only the arena entry index and the executor
/// pointer where the actual transport handle lives. Mirrors
/// `ActionClientInternal`.
///
/// Phase 87.5: `#[repr(C)]` gives deterministic layout so cbindgen can
/// emit this struct into the C header.
#[repr(C)]
pub struct ServiceClientInternal {
    /// Arena entry index. -1 means not registered with any executor yet.
    pub arena_entry_index: i32,
    /// Pointer to the Rust executor that owns the arena entry.
    pub executor_ptr: *mut c_void,
    /// Default timeout used by `nros_client_call`.
    pub timeout_ms: u32,
}

impl ServiceClientInternal {
    pub const fn new() -> Self {
        Self {
            arena_entry_index: -1,
            executor_ptr: ptr::null_mut(),
            timeout_ms: NROS_DEFAULT_SERVICE_TIMEOUT_MS,
        }
    }
}

impl Default for ServiceClientInternal {
    fn default() -> Self {
        Self::new()
    }
}

/// Client state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_client_state_t {
    /// Not initialized
    NROS_CLIENT_STATE_UNINITIALIZED = 0,
    /// Initialized — metadata stored, *not yet registered with an executor*.
    NROS_CLIENT_STATE_INITIALIZED = 1,
    /// Registered with an executor and ready for `send_request_async` / `call`.
    NROS_CLIENT_STATE_REGISTERED = 2,
    /// Shutdown
    NROS_CLIENT_STATE_SHUTDOWN = 3,
}

/// Service client structure.
#[repr(C)]
pub struct nros_client_t {
    /// Current state
    pub state: nros_client_state_t,
    /// Service name storage
    pub service_name: [u8; MAX_SERVICE_NAME_LEN],
    /// Service name length
    pub service_name_len: usize,
    /// Type name storage
    pub type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    pub type_name_len: usize,
    /// Type hash storage
    pub type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    pub type_hash_len: usize,
    /// User response callback, fired from `nros_executor_spin_some` when
    /// a response to a previously-sent async request arrives.
    pub response_callback: nros_response_callback_t,
    /// User context pointer passed to `response_callback`.
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Internal state (arena entry index + executor pointer + timeout).
    /// Phase 87.5: now a typed `#[repr(C)]` field.
    pub _internal: ServiceClientInternal,
}

impl Default for nros_client_t {
    fn default() -> Self {
        Self {
            state: nros_client_state_t::NROS_CLIENT_STATE_UNINITIALIZED,
            service_name: [0u8; MAX_SERVICE_NAME_LEN],
            service_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            response_callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ServiceClientInternal::new(),
        }
    }
}

/// Get a zero-initialized client.
#[unsafe(no_mangle)]
pub extern "C" fn nros_client_get_zero_initialized() -> nros_client_t {
    nros_client_t::default()
}

/// Initialize a service client (Phase 82: metadata-only).
///
/// Stores the service name/type metadata and a `ServiceClientInternal`
/// blob; the actual transport handle (`RmwServiceClient`) is created
/// later by `nros_executor_add_client`. This deferred lifecycle matches
/// `nros_service_init` (server side) and the action client.
///
/// # Parameters
/// * `client` - Pointer to a zero-initialized client
/// * `node` - Pointer to an initialized node
/// * `type_info` - Pointer to service type information
/// * `service_name` - Service name (null-terminated string)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any required pointer is NULL
/// * `NROS_RET_NOT_INIT` if node is not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_init(
    client: *mut nros_client_t,
    node: *const nros_node_t,
    type_info: *const nros_service_type_t,
    service_name: *const c_char,
) -> nros_ret_t {
    validate_not_null!(client, node, type_info, service_name);

    let client = &mut *client;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        client,
        nros_client_state_t::NROS_CLIENT_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy service name (required — empty rejected)
    client.service_name_len = crate::util::copy_cstr_into(service_name, &mut client.service_name);
    if client.service_name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    client.type_name_len =
        crate::util::copy_cstr_into(type_info.type_name, &mut client.type_name);
    client.type_hash_len =
        crate::util::copy_cstr_into(type_info.type_hash, &mut client.type_hash);

    // Store node pointer + zero callback fields
    client.node = node;
    client.response_callback = None;
    client.context = ptr::null_mut();

    // Initialise the internal state (executor_ptr null until registration).
    client._internal = ServiceClientInternal::new();

    client.state = nros_client_state_t::NROS_CLIENT_STATE_INITIALIZED;
    NROS_RET_OK
}

/// Finalize a service client.
///
/// Phase 82: the underlying transport handle lives in the executor's
/// arena and is dropped automatically when the executor is finalised.
/// This function only resets the C-side metadata + internal blob.
///
/// # Parameters
/// * `client` - Pointer to an initialized client
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if client is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_fini(client: *mut nros_client_t) -> nros_ret_t {
    validate_not_null!(client);

    let client = &mut *client;

    if client.state == nros_client_state_t::NROS_CLIENT_STATE_UNINITIALIZED
        || client.state == nros_client_state_t::NROS_CLIENT_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    // Reset the inline ServiceClientInternal. The RmwServiceClient lives
    // in the executor's arena and is freed when the executor is destroyed.
    client._internal = ServiceClientInternal::new();
    client.response_callback = None;
    client.context = ptr::null_mut();
    client.node = ptr::null();
    client.state = nros_client_state_t::NROS_CLIENT_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Service Client async pair + setters (Phase 82)
// ============================================================================

/// Response trampoline registered with the executor's arena entry.
///
/// Reads the user's `response_callback` from the `nros_client_t` struct
/// at invocation time so the blocking wrapper (`nros_client_call`) can
/// install a one-shot callback after registration.
///
/// # Safety
/// `context` must point to a live `nros_client_t`.
pub(crate) unsafe extern "C" fn client_response_trampoline(
    response: *const u8,
    response_len: usize,
    context: *mut c_void,
) {
    let client = &*(context as *const nros_client_t);
    if let Some(cb) = client.response_callback {
        cb(response, response_len, client.context);
    }
}

/// Set the response callback fired by `nros_executor_spin_some` when an
/// async request has its reply delivered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_set_response_callback(
    client: *mut nros_client_t,
    callback: nros_response_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(client);
    let client = &mut *client;
    client.response_callback = callback;
    client.context = context;
    NROS_RET_OK
}

/// Set the default timeout used by `nros_client_call`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_set_timeout(
    client: *mut nros_client_t,
    timeout_ms: u32,
) -> nros_ret_t {
    validate_not_null!(client);
    let client = &mut *client;
    client._internal.timeout_ms = timeout_ms;
    NROS_RET_OK
}

/// Block until a matching service server is discoverable, or `timeout_ms`
/// elapses. Mirrors `rclcpp::ClientBase::wait_for_service` and the
/// public Rust `Client::wait_for_service`.
///
/// The client must already have been registered with the executor via
/// `nros_executor_add_client`. Internally fires liveliness queries
/// against the matching service-server's wildcard liveliness keyexpr
/// and spins the executor cooperatively while the probe is in flight.
/// 1-second per-probe timeout, looped until either a token reply lands
/// or the outer wall-clock budget expires — see the Rust API for the
/// rationale (a single liveliness_get samples the router's current
/// token list and terminates, so a server that comes up after we
/// start waiting needs to be re-probed).
///
/// # Returns
/// * `NROS_RET_OK` — server is visible (proceed with `nros_client_call`).
/// * `NROS_RET_TIMEOUT` — `timeout_ms` elapsed without seeing a token.
/// * `NROS_RET_NOT_INIT` — client not registered with an executor.
/// * `NROS_RET_ERROR` — transport-level failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_wait_for_service(
    client: *mut nros_client_t,
    timeout_ms: u32,
) -> nros_ret_t {
    validate_not_null!(client);

    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
    {
        use nros_node::ServiceClientTrait;

        let client_ref = &mut *client;
        if client_ref.state != nros_client_state_t::NROS_CLIENT_STATE_REGISTERED {
            return NROS_RET_NOT_INIT;
        }
        let internal = &mut client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return NROS_RET_NOT_INIT;
        }

        let exec_t = &mut *(internal.executor_ptr as *mut nros_executor_t);
        if exec_t.in_dispatch {
            return NROS_RET_REENTRANT;
        }
        let executor = exec_t as *mut nros_executor_t;

        // Latched fast-path: if a previous wait already proved the
        // server is reachable, don't re-probe.
        {
            let exec = crate::executor::get_executor(&mut exec_t._opaque);
            let entry = match exec.service_client_entry_mut(internal.arena_entry_index as usize) {
                Some(e) => e,
                None => return NROS_RET_NOT_INIT,
            };
            if entry.handle.is_server_ready() {
                return NROS_RET_OK;
            }
        }

        // Per-probe / outer budget. Mirrors `Client::wait_for_service` in
        // packages/core/nros-node/src/executor/handles.rs.
        const PROBE_TIMEOUT_MS: u32 = 1000;
        let start_ns = crate::platform::get_time_ns();
        let timeout_ns: u64 = (timeout_ms as u64).saturating_mul(1_000_000);
        loop {
            // Re-borrow each iteration to avoid holding `entry` across
            // the executor spin (which itself touches the arena).
            {
                let exec = crate::executor::get_executor(&mut exec_t._opaque);
                let entry = match exec.service_client_entry_mut(internal.arena_entry_index as usize)
                {
                    Some(e) => e,
                    None => return NROS_RET_NOT_INIT,
                };
                if let Err(_) = entry.handle.start_server_discovery(PROBE_TIMEOUT_MS) {
                    return NROS_RET_ERROR;
                }
            }

            // Drain this probe to completion (token reply or empty FINAL).
            loop {
                crate::executor::nros_executor_spin_some(executor, 10_000_000);

                let exec = crate::executor::get_executor(&mut exec_t._opaque);
                let entry = match exec.service_client_entry_mut(internal.arena_entry_index as usize)
                {
                    Some(e) => e,
                    None => return NROS_RET_NOT_INIT,
                };
                match entry.handle.poll_server_discovery() {
                    Ok(Some(true)) => return NROS_RET_OK,
                    Ok(Some(false)) => break, // probe finished empty — re-issue
                    Ok(None) => {}            // still in flight
                    Err(_) => return NROS_RET_ERROR,
                }

                let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
                if elapsed_ns >= timeout_ns {
                    return NROS_RET_TIMEOUT;
                }
            }

            let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
            if elapsed_ns >= timeout_ns {
                return NROS_RET_TIMEOUT;
            }
        }
    }

    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce")))]
    {
        let _ = (client, timeout_ms);
        NROS_RET_OK
    }
}

/// Non-blocking snapshot of whether a matching service server is
/// currently visible. Mirrors `rclcpp::ClientBase::service_is_ready`
/// and rcl's `rcl_service_server_is_available`. Returns `false` when
/// the client isn't registered with an executor or the backend lacks
/// liveliness discovery (in which case use `nros_client_wait_for_service`
/// instead, which handles those cases conservatively).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_service_is_ready(client: *const nros_client_t) -> bool {
    if client.is_null() {
        return false;
    }
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
    {
        use nros_node::ServiceClientTrait;

        let client_ref = &*client;
        if client_ref.state != nros_client_state_t::NROS_CLIENT_STATE_REGISTERED {
            return false;
        }
        let internal = &client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return false;
        }
        let exec_t = &mut *(internal.executor_ptr as *mut nros_executor_t);
        let exec = crate::executor::get_executor(&mut exec_t._opaque);
        match exec.service_client_entry_mut(internal.arena_entry_index as usize) {
            Some(entry) => entry.handle.is_server_ready(),
            None => false,
        }
    }
    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce")))]
    {
        let _ = client;
        true
    }
}

/// Send a service request asynchronously (Phase 82).
///
/// Non-blocking. The reply is delivered via the registered
/// `response_callback` during `nros_executor_spin_some`. The user must
/// have previously registered the client with `nros_executor_add_client`.
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_NOT_INIT` if the client is not registered with an executor
/// * `NROS_RET_BAD_SEQUENCE` if a previous request is still pending
/// * `NROS_RET_ERROR` on transport failure
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_send_request_async(
    client: *mut nros_client_t,
    request_data: *const u8,
    request_len: usize,
) -> nros_ret_t {
    validate_not_null!(client, request_data);

    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
    {
        use nros_node::ServiceClientTrait;

        let client_ref = &mut *client;
        if client_ref.state != nros_client_state_t::NROS_CLIENT_STATE_REGISTERED {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return NROS_RET_NOT_INIT;
        }

        let exec_t = &mut *(internal.executor_ptr as *mut nros_executor_t);
        let exec = crate::executor::get_executor(&mut exec_t._opaque);
        let entry = match exec.service_client_entry_mut(internal.arena_entry_index as usize) {
            Some(e) => e,
            None => return NROS_RET_NOT_INIT,
        };
        if entry.pending {
            return NROS_RET_BAD_SEQUENCE;
        }

        let request = core::slice::from_raw_parts(request_data, request_len);
        // Clear the ready flag before sending so we don't pick up a
        // stale wake from a previous request.
        entry
            .reply_ready
            .store(false, core::sync::atomic::Ordering::Release);
        match entry.handle.send_request_raw(request) {
            Ok(()) => {
                entry.pending = true;
                // Register a waker that sets reply_ready when the
                // transport delivers the reply. This replaces blind
                // polling of get_check on every spin tick.
                let waker = atomic_bool_waker(&entry.reply_ready);
                entry.handle.register_waker(&waker);
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce")))]
    {
        let _ = (client, request_data, request_len);
        NROS_RET_ERROR
    }
}

/// Poll for the reply to the most recently sent async request.
///
/// # Returns
/// * `NROS_RET_OK` if the reply was filled into `response_data`
/// * `NROS_RET_TRY_AGAIN` if no reply yet (caller should spin and retry)
/// * `NROS_RET_NOT_INIT` if the client isn't registered or has no pending request
/// * `NROS_RET_ERROR` on transport failure
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_try_recv_response(
    client: *mut nros_client_t,
    response_data: *mut u8,
    response_capacity: usize,
    response_len: *mut usize,
) -> nros_ret_t {
    validate_not_null!(client, response_data, response_len);

    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
    {
        use nros_node::ServiceClientTrait;

        let client_ref = &mut *client;
        if client_ref.state != nros_client_state_t::NROS_CLIENT_STATE_REGISTERED {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return NROS_RET_NOT_INIT;
        }

        let exec_t = &mut *(internal.executor_ptr as *mut nros_executor_t);
        let exec = crate::executor::get_executor(&mut exec_t._opaque);
        let entry = match exec.service_client_entry_mut(internal.arena_entry_index as usize) {
            Some(e) => e,
            None => return NROS_RET_NOT_INIT,
        };
        if !entry.pending {
            return NROS_RET_NOT_INIT;
        }

        let buf = core::slice::from_raw_parts_mut(response_data, response_capacity);
        match entry.handle.try_recv_reply_raw(buf) {
            Ok(Some(len)) => {
                entry.pending = false;
                *response_len = len;
                NROS_RET_OK
            }
            Ok(None) => NROS_RET_TRY_AGAIN,
            Err(_) => {
                entry.pending = false;
                NROS_RET_ERROR
            }
        }
    }

    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce")))]
    {
        let _ = (client, response_data, response_capacity, response_len);
        NROS_RET_ERROR
    }
}

/// Call a service (blocking convenience over the async pair).
///
/// Phase 82: signature unchanged, but no longer blocks at the transport
/// layer. Internally calls `nros_client_send_request_async` and spins
/// the registered executor via `nros_executor_spin_some` until the
/// response arrives or the client's `timeout_ms` elapses. The client
/// must have been registered with `nros_executor_add_client`.
///
/// # Parameters
/// * `client` - Pointer to a registered client
/// * `request_data` - CDR-serialized request data
/// * `request_len` - Length of request data
/// * `response_data` - Buffer to receive CDR-serialized response
/// * `response_capacity` - Capacity of response buffer
/// * `response_len` - Output: actual length of response data
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NROS_RET_NOT_INIT` if the client isn't registered with an executor
/// * `NROS_RET_TIMEOUT` if no response within `timeout_ms`
/// * `NROS_RET_ERROR` on call failure
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub unsafe extern "C" fn nros_client_call(
    client: *mut nros_client_t,
    request_data: *const u8,
    request_len: usize,
    response_data: *mut u8,
    response_capacity: usize,
    response_len: *mut usize,
) -> nros_ret_t {
    validate_not_null!(client, request_data, response_data, response_len);

    let client_ref = &mut *client;
    if client_ref.state != nros_client_state_t::NROS_CLIENT_STATE_REGISTERED {
        return NROS_RET_NOT_INIT;
    }

    let executor_ptr = client_ref._internal.executor_ptr;
    let timeout_ms = client_ref._internal.timeout_ms;
    if executor_ptr.is_null() {
        return NROS_RET_NOT_INIT;
    }

    // Reentrancy guard: nros_client_call spins the executor internally,
    // so it must not be called from inside a dispatch callback.
    let exec_t = &*(executor_ptr as *const nros_executor_t);
    if exec_t.in_dispatch {
        return NROS_RET_REENTRANT;
    }

    // One-shot blocking response capture. Static is fine because
    // nros_client_call is non-reentrant by design (callable only from
    // outside spin_some; the reentrancy guard in 82.8 will enforce it).
    static mut BLK_DONE: i32 = -1;
    static mut BLK_LEN: usize = 0;
    static mut BLK_BUF: [u8; 4096] = [0u8; 4096];
    BLK_DONE = -1;
    BLK_LEN = 0;

    let orig_cb = client_ref.response_callback;
    let orig_ctx = client_ref.context;

    unsafe extern "C" fn blocking_response_cb(data: *const u8, len: usize, _ctx: *mut c_void) {
        let copy = len.min(BLK_BUF.len());
        core::ptr::copy_nonoverlapping(data, BLK_BUF.as_mut_ptr(), copy);
        BLK_LEN = copy;
        BLK_DONE = 1;
    }
    client_ref.response_callback = Some(blocking_response_cb);

    let send = nros_client_send_request_async(client, request_data, request_len);
    if send != NROS_RET_OK {
        client_ref.response_callback = orig_cb;
        client_ref.context = orig_ctx;
        return send;
    }

    // Spin: drive I/O then dispatch arena entries. On single-threaded
    // transports (smoltcp/NuttX), drive_io reads from the socket. On
    // multi-threaded (POSIX), the background thread handles I/O and
    // the waker signals reply_ready when the response arrives.
    //
    // Phase 89.2: wall-clock budgeting instead of `max_spins = timeout_ms/10`.
    // On multi-threaded zpico backends (POSIX/Zephyr) the condvar-wait in
    // `zpico_spin_once(10)` can return early on any incoming frame
    // (keep-alives, discovery gossip, …), so a pure iteration count can
    // burn through the nominal timeout in milliseconds and return TIMEOUT
    // before the reply actually has a chance to arrive. Budget by the clock.
    let executor = executor_ptr as *mut nros_executor_t;
    let start_ns = crate::platform::get_time_ns();
    let timeout_ns: u64 = (timeout_ms as u64).saturating_mul(1_000_000);
    loop {
        crate::executor::nros_executor_spin_some(executor, 10_000_000);
        if BLK_DONE >= 0 {
            client_ref.response_callback = orig_cb;
            client_ref.context = orig_ctx;
            if BLK_LEN > response_capacity {
                return NROS_RET_ERROR;
            }
            core::ptr::copy_nonoverlapping(BLK_BUF.as_ptr(), response_data, BLK_LEN);
            *response_len = BLK_LEN;
            return NROS_RET_OK;
        }
        let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
        if elapsed_ns >= timeout_ns {
            break;
        }
    }
    client_ref.response_callback = orig_cb;
    client_ref.context = orig_ctx;

    // Phase 89.12: clear `entry.pending` (set by `nros_client_send_request_async`
    // at line ~763) so the next `nros_client_call` doesn't bounce off
    // NROS_RET_BAD_SEQUENCE. Without this, a single slow-first-RPC
    // timeout cascades every subsequent blocking call on the same
    // client — which is exactly what NuttX `lang_2_C` rtos_e2e flakes
    // hit on QEMU cold-boot: call [1] times out because the server
    // queryable isn't ready, calls [2–4] all return BAD_SEQUENCE, the
    // test sees 0 responses and fails even though the server came up
    // fine. Symmetrical to the RAII-style reset on Rust's `Promise`
    // drop path (handles.rs::Promise::try_recv clears `in_flight` on
    // successful reception) — we reset here on the timeout path.
    //
    // Semantic note: if the late reply for the timed-out call arrives
    // before the caller fires another request, it will be picked up
    // by the next `nros_client_try_recv_response` / spin dispatch.
    // That's a known "stale reply" quirk of single-slot clients; the
    // caller either tolerates it (match on returned seq) or resets
    // the slot explicitly. The previous behaviour — silently jamming
    // every subsequent call — was strictly worse.
    let internal = &mut client_ref._internal;
    if !internal.executor_ptr.is_null() && internal.arena_entry_index >= 0 {
        let exec_t = &mut *(internal.executor_ptr as *mut nros_executor_t);
        let exec = crate::executor::get_executor(&mut exec_t._opaque);
        if let Some(entry) =
            exec.service_client_entry_mut(internal.arena_entry_index as usize)
        {
            entry.pending = false;
            entry
                .reply_ready
                .store(false, core::sync::atomic::Ordering::Release);
        }
    }

    NROS_RET_TIMEOUT
}

/// Get the service name of a client.
///
/// # Parameters
/// * `client` - Pointer to a client
///
/// # Returns
/// * Pointer to service name (null-terminated), or NULL if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_get_service_name(
    client: *const nros_client_t,
) -> *const c_char {
    if client.is_null() {
        return ptr::null();
    }

    let client = &*client;
    if client.state != nros_client_state_t::NROS_CLIENT_STATE_INITIALIZED {
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
/// * `true` if valid, `false` if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_client_is_valid(client: *const nros_client_t) -> bool {
    if client.is_null() {
        return false;
    }

    let client = &*client;
    client.state == nros_client_state_t::NROS_CLIENT_STATE_INITIALIZED
}

// ============================================================================
// Kani Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;
    use core::ptr;

    // Helper to create a dummy type_info
    fn dummy_service_type() -> nros_service_type_t {
        let type_name = b"example_interfaces::srv::dds_::AddTwoInts_\0";
        let type_hash = b"RIHS01_test\0";
        nros_service_type_t {
            type_name: type_name.as_ptr() as *const core::ffi::c_char,
            type_hash: type_hash.as_ptr() as *const core::ffi::c_char,
        }
    }

    // Helper callback for service init
    unsafe extern "C" fn dummy_callback(
        _req: *const u8,
        _req_len: usize,
        _resp: *mut u8,
        _resp_cap: usize,
        _resp_len: *mut usize,
        _ctx: *mut core::ffi::c_void,
    ) -> bool {
        true
    }

    // -- Service Server Harnesses --

    #[kani::proof]
    #[kani::unwind(5)]
    fn service_init_null_ptrs() {
        let svc_name = b"/add_two_ints\0";
        let type_info = dummy_service_type();
        let mut node = crate::node::nros_node_get_zero_initialized();

        // NULL service
        assert_eq!(
            unsafe {
                nros_service_init(
                    ptr::null_mut(),
                    &node,
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                    Some(dummy_callback),
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL node
        let mut svc = nros_service_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_service_init(
                    &mut svc,
                    ptr::null(),
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                    Some(dummy_callback),
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL type_info
        let mut svc = nros_service_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_service_init(
                    &mut svc,
                    &node,
                    ptr::null(),
                    svc_name.as_ptr() as *const core::ffi::c_char,
                    Some(dummy_callback),
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL service_name
        let mut svc = nros_service_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_service_init(
                    &mut svc,
                    &node,
                    &type_info,
                    ptr::null(),
                    Some(dummy_callback),
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn service_init_none_callback() {
        let svc_name = b"/add_two_ints\0";
        let type_info = dummy_service_type();
        let node = crate::node::nros_node_get_zero_initialized();

        let mut svc = nros_service_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_service_init(
                    &mut svc,
                    &node,
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn service_init_uninit_node() {
        let svc_name = b"/add_two_ints\0";
        let type_info = dummy_service_type();
        let node = crate::node::nros_node_get_zero_initialized();

        // Node is UNINITIALIZED → NOT_INIT
        let mut svc = nros_service_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_service_init(
                    &mut svc,
                    &node,
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                    Some(dummy_callback),
                    ptr::null_mut(),
                )
            },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn service_zero_initialized_state() {
        let svc = nros_service_get_zero_initialized();
        assert_eq!(
            svc.state,
            nros_service_state_t::NROS_SERVICE_STATE_UNINITIALIZED,
        );
        assert!(svc.node.is_null());
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn service_fini_null_safety() {
        // NULL → INVALID_ARGUMENT
        assert_eq!(
            unsafe { nros_service_fini(ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // UNINITIALIZED → NOT_INIT
        let mut svc = nros_service_get_zero_initialized();
        assert_eq!(unsafe { nros_service_fini(&mut svc) }, NROS_RET_NOT_INIT,);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn service_double_init_rejected() {
        let svc_name = b"/add_two_ints\0";
        let type_info = dummy_service_type();
        let mut node = crate::node::nros_node_get_zero_initialized();
        // Manually set node to initialized state for this test
        node.state = crate::node::nros_node_state_t::NROS_NODE_STATE_INITIALIZED;

        let mut svc = nros_service_get_zero_initialized();
        // First init succeeds (metadata only)
        let ret = unsafe {
            nros_service_init(
                &mut svc,
                &node,
                &type_info,
                svc_name.as_ptr() as *const core::ffi::c_char,
                Some(dummy_callback),
                ptr::null_mut(),
            )
        };
        assert_eq!(ret, NROS_RET_OK);

        // Second init → BAD_SEQUENCE
        assert_eq!(
            unsafe {
                nros_service_init(
                    &mut svc,
                    &node,
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                    Some(dummy_callback),
                    ptr::null_mut(),
                )
            },
            NROS_RET_BAD_SEQUENCE,
        );
    }

    // -- Service Client Harnesses --

    #[kani::proof]
    #[kani::unwind(5)]
    fn client_init_null_ptrs() {
        let svc_name = b"/add_two_ints\0";
        let type_info = dummy_service_type();
        let node = crate::node::nros_node_get_zero_initialized();

        // NULL client
        assert_eq!(
            unsafe {
                nros_client_init(
                    ptr::null_mut(),
                    &node,
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL node
        let mut client = nros_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_client_init(
                    &mut client,
                    ptr::null(),
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL type_info
        let mut client = nros_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_client_init(
                    &mut client,
                    &node,
                    ptr::null(),
                    svc_name.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL service_name
        let mut client = nros_client_get_zero_initialized();
        assert_eq!(
            unsafe { nros_client_init(&mut client, &node, &type_info, ptr::null()) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn client_init_uninit_node() {
        let svc_name = b"/add_two_ints\0";
        let type_info = dummy_service_type();
        let node = crate::node::nros_node_get_zero_initialized();

        let mut client = nros_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_client_init(
                    &mut client,
                    &node,
                    &type_info,
                    svc_name.as_ptr() as *const core::ffi::c_char,
                )
            },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn client_zero_initialized_state() {
        let client = nros_client_get_zero_initialized();
        assert_eq!(
            client.state,
            nros_client_state_t::NROS_CLIENT_STATE_UNINITIALIZED,
        );
        assert!(client.node.is_null());
        assert!(client._internal.iter().all(|&v| v == 0));
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn client_fini_null_safety() {
        // NULL → INVALID_ARGUMENT
        assert_eq!(
            unsafe { nros_client_fini(ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // UNINITIALIZED → NOT_INIT
        let mut client = nros_client_get_zero_initialized();
        assert_eq!(unsafe { nros_client_fini(&mut client) }, NROS_RET_NOT_INIT,);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn client_call_null_safety() {
        let req = [0u8; 8];
        let mut resp = [0u8; 8];
        let mut resp_len: usize = 0;

        // NULL client
        assert_eq!(
            unsafe {
                nros_client_call(
                    ptr::null_mut(),
                    req.as_ptr(),
                    req.len(),
                    resp.as_mut_ptr(),
                    resp.len(),
                    &mut resp_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL request_data
        let mut client = nros_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_client_call(
                    &mut client,
                    ptr::null(),
                    0,
                    resp.as_mut_ptr(),
                    resp.len(),
                    &mut resp_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL response_data
        assert_eq!(
            unsafe {
                nros_client_call(
                    &mut client,
                    req.as_ptr(),
                    req.len(),
                    ptr::null_mut(),
                    0,
                    &mut resp_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL response_len
        assert_eq!(
            unsafe {
                nros_client_call(
                    &mut client,
                    req.as_ptr(),
                    req.len(),
                    resp.as_mut_ptr(),
                    resp.len(),
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn client_call_reentrant_rejected() {
        let svc_name = b"/add_two_ints\0";
        let type_info = dummy_service_type();
        let mut node = crate::node::nros_node_get_zero_initialized();
        node.state = crate::node::nros_node_state_t::NROS_NODE_STATE_INITIALIZED;

        let mut client = nros_client_get_zero_initialized();
        let ret = unsafe {
            nros_client_init(
                &mut client,
                &node,
                &type_info,
                svc_name.as_ptr() as *const core::ffi::c_char,
            )
        };
        assert_eq!(ret, NROS_RET_OK);

        // Simulate registration: set state to REGISTERED and stash
        // a pointer to a fake executor with in_dispatch = true.
        let mut executor = crate::executor::nros_executor_get_zero_initialized();
        executor.state = crate::executor::nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
        executor.in_dispatch = true;

        client.state = nros_client_state_t::NROS_CLIENT_STATE_REGISTERED;
        client._internal.executor_ptr = &mut executor as *mut _ as *mut core::ffi::c_void;
        client._internal.timeout_ms = 5000;

        let req = [0u8; 8];
        let mut resp = [0u8; 8];
        let mut resp_len: usize = 0;

        assert_eq!(
            unsafe {
                nros_client_call(
                    &mut client,
                    req.as_ptr(),
                    req.len(),
                    resp.as_mut_ptr(),
                    resp.len(),
                    &mut resp_len,
                )
            },
            NROS_RET_REENTRANT,
        );
    }

    // -- Name Getter Harnesses --

    #[kani::proof]
    #[kani::unwind(5)]
    fn service_name_getter_null() {
        let result = unsafe { nros_service_get_service_name(ptr::null()) };
        assert!(result.is_null());
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn client_name_getter_null() {
        let result = unsafe { nros_client_get_service_name(ptr::null()) };
        assert!(result.is_null());
    }
}
