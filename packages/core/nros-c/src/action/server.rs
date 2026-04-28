//! Action server implementation.

use core::ffi::c_void;
use core::ptr;

use nros::GoalId;
use nros::cdr::{CDR_HEADER_LEN, strip_cdr_header, write_cdr_le_header};

use super::common::*;
use crate::constants::{MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};

/// CDR sequence<uint8, 16> length prefix (4 bytes) in front of the UUID bytes.
/// See [`nros_node::GoalId`] encoding in `CLAUDE.md`.
const GOAL_ID_SEQ_PREFIX_LEN: usize = 4;

/// Bytes of CDR framing that precede the goal payload in a send_goal request:
/// CDR encapsulation header + GoalId sequence length prefix + UUID.
const GOAL_REQUEST_FRAMING_LEN: usize = CDR_HEADER_LEN + GOAL_ID_SEQ_PREFIX_LEN + GoalId::UUID_LEN;

// ============================================================================
// Action Server
// ============================================================================

/// Action server state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_action_server_state_t {
    /// Not initialized
    NROS_ACTION_SERVER_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_ACTION_SERVER_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_ACTION_SERVER_STATE_SHUTDOWN = 2,
}

/// Action server structure.
#[repr(C)]
pub struct nros_action_server_t {
    /// Current state
    pub state: nros_action_server_state_t,
    /// Action name storage
    pub action_name: [u8; MAX_ACTION_NAME_LEN],
    /// Action name length
    pub action_name_len: usize,
    /// Type name storage
    pub type_name: [u8; MAX_TYPE_NAME_LEN],
    /// Type name length
    pub type_name_len: usize,
    /// Type hash storage
    pub type_hash: [u8; MAX_TYPE_HASH_LEN],
    /// Type hash length
    pub type_hash_len: usize,
    /// Goal callback
    pub goal_callback: nros_goal_callback_t,
    /// Cancel callback
    pub cancel_callback: nros_cancel_callback_t,
    /// Accepted callback
    pub accepted_callback: nros_accepted_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Internal state — set by `nros_executor_add_action_server`.
    /// Phase 87.5: typed `#[repr(C)]` field, no longer an opaque blob.
    pub _internal: ActionServerInternal,
}

impl Default for nros_action_server_t {
    fn default() -> Self {
        Self {
            state: nros_action_server_state_t::NROS_ACTION_SERVER_STATE_UNINITIALIZED,
            action_name: [0u8; MAX_ACTION_NAME_LEN],
            action_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            goal_callback: None,
            cancel_callback: None,
            accepted_callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ActionServerInternal::invalid_default(),
        }
    }
}

// ============================================================================
// Internal implementation (delegates to nros-node executor)
// ============================================================================

/// Internal state created during executor registration.
///
/// Holds the action server handle and C callback pointers needed by
/// the goal/cancel trampolines.
///
/// Phase 87.5: `#[repr(C)]` so cbindgen sees this struct directly.
/// `handle` was `Option<ActionServerRawHandle>` previously; now it is
/// always present, with the sentinel `INVALID_ENTRY_INDEX` indicating
/// "not registered yet". Use `is_handle_set()` to check.
#[repr(C)]
pub struct ActionServerInternal {
    /// Handle returned by executor registration. `entry_index ==
    /// INVALID_ENTRY_INDEX` until registration completes.
    pub handle: nros_node::ActionServerRawHandle,
    /// Pointer to the internal Rust executor (`CExecutor`).
    pub executor_ptr: *mut c_void,
    /// C goal callback from init. Required.
    pub c_goal_callback: unsafe extern "C" fn(
        *mut nros_action_server_t,
        *const nros_goal_handle_t,
        *const u8,
        usize,
        *mut c_void,
    ) -> nros_goal_response_t,
    /// C cancel callback from init (may be None).
    pub c_cancel_callback: nros_cancel_callback_t,
    /// C accepted callback from init (may be None).
    pub c_accepted_callback: nros_accepted_callback_t,
    /// C user context from init.
    pub c_context: *mut c_void,
    /// Pointer back to the C action server struct.
    pub server_ptr: *mut nros_action_server_t,
}

impl ActionServerInternal {
    /// `true` once `handle` has been populated by executor registration.
    #[inline]
    pub fn is_handle_set(&self) -> bool {
        !self.handle.is_invalid()
    }

    /// Construct a zero-initialised internal — sentinel handle, null pointers,
    /// no callbacks. Used for the default state of `nros_action_server_t._internal`.
    pub fn invalid_default() -> Self {
        Self {
            handle: nros_node::ActionServerRawHandle::invalid(),
            executor_ptr: ptr::null_mut(),
            c_goal_callback: dummy_goal_callback,
            c_cancel_callback: None,
            c_accepted_callback: None,
            c_context: ptr::null_mut(),
            server_ptr: ptr::null_mut(),
        }
    }
}

impl Default for ActionServerInternal {
    fn default() -> Self {
        Self::invalid_default()
    }
}

/// Stub for the required `c_goal_callback` field when the internal is in
/// its uninitialised state. Never called — the dispatch path checks
/// `is_handle_set()` first.
unsafe extern "C" fn dummy_goal_callback(
    _server: *mut nros_action_server_t,
    _goal: *const nros_goal_handle_t,
    _data: *const u8,
    _len: usize,
    _ctx: *mut c_void,
) -> nros_goal_response_t {
    nros_goal_response_t::NROS_GOAL_REJECT
}

/// Build a stack-local `nros_goal_handle_t` from an arena-supplied UUID.
///
/// The handle is a pure ID card (`{uuid}`), so the trampolines can hand
/// user callbacks a `*const nros_goal_handle_t` that's valid for the
/// duration of the callback. Users copy the handle by value if they need
/// the UUID beyond the callback's lifetime.
unsafe fn handle_from_goal_id(goal_id: *const nros_node::GoalId) -> nros_goal_handle_t {
    nros_goal_handle_t {
        uuid: nros_goal_uuid_t {
            uuid: (*goal_id).uuid,
        },
    }
}

/// Goal callback trampoline for nros-node.
///
/// Builds a stack-local `nros_goal_handle_t` from the incoming goal_id
/// and forwards to the user's `c_goal_callback` with `(server, goal,
/// request, len, ctx)`. The handle lives on this stack frame; users that
/// need to reference the goal later copy it by value.
pub(crate) unsafe extern "C" fn goal_callback_trampoline(
    goal_id: *const nros_node::GoalId,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut c_void,
) -> nros_node::GoalResponse {
    let internal = &*(context as *const ActionServerInternal);
    let goal_handle = handle_from_goal_id(goal_id);

    // goal_data contains the full CDR request: [CDR_HDR][GoalId seq][UUID][goal_fields].
    // The C callback expects CDR-encoded goal data: [CDR_HDR][goal_fields].
    // Extract goal fields (after CDR header + GoalId) and prepend a CDR header.
    let goal_slice = core::slice::from_raw_parts(goal_data, goal_len);

    // Build [CDR_HEADER][goal_fields] on the stack (must outlive the callback)
    let mut cb_buf = [0u8; 512];
    let (cb_ptr, cb_len) = if goal_len > GOAL_REQUEST_FRAMING_LEN {
        let fields = &goal_slice[GOAL_REQUEST_FRAMING_LEN..];
        let payload = write_cdr_le_header(&mut cb_buf).expect("cb_buf >= CDR_HEADER_LEN");
        let copy_len = fields.len().min(payload.len());
        payload[..copy_len].copy_from_slice(&fields[..copy_len]);
        (cb_buf.as_ptr(), CDR_HEADER_LEN + copy_len)
    } else {
        (goal_data, goal_len)
    };

    let c_response = (internal.c_goal_callback)(
        internal.server_ptr,
        &goal_handle,
        cb_ptr,
        cb_len,
        internal.c_context,
    );

    match c_response {
        nros_goal_response_t::NROS_GOAL_REJECT => nros_node::GoalResponse::Reject,
        nros_goal_response_t::NROS_GOAL_ACCEPT_AND_EXECUTE => {
            nros_node::GoalResponse::AcceptAndExecute
        }
        nros_goal_response_t::NROS_GOAL_ACCEPT_AND_DEFER => nros_node::GoalResponse::AcceptAndDefer,
    }
}

/// Post-accept trampoline for nros-node.
///
/// Invoked by the arena *after* `ActionServerCore::accept_goal` has sent
/// the accept reply to the client. Forwards to the user's
/// `c_accepted_callback` with a stack-local handle.
pub(crate) unsafe extern "C" fn accepted_callback_trampoline(
    goal_id: *const nros_node::GoalId,
    context: *mut c_void,
) {
    let internal = &*(context as *const ActionServerInternal);
    if let Some(cb) = internal.c_accepted_callback {
        let goal_handle = handle_from_goal_id(goal_id);
        cb(internal.server_ptr, &goal_handle, internal.c_context);
    }
}

/// Cancel callback trampoline for nros-node.
///
/// Forwards to the user's `c_cancel_callback` with a stack-local handle.
/// Status transitions (CANCELING, etc.) are performed by the arena in
/// `ActionServerCore`; this trampoline only translates the ABI.
pub(crate) unsafe extern "C" fn cancel_callback_trampoline(
    goal_id: *const nros_node::GoalId,
    _status: nros_node::GoalStatus,
    context: *mut c_void,
) -> nros_node::CancelResponse {
    let internal = &*(context as *const ActionServerInternal);

    let Some(cb) = internal.c_cancel_callback else {
        // No cancel callback — accept by default.
        return nros_node::CancelResponse::Ok;
    };

    let goal_handle = handle_from_goal_id(goal_id);
    let c_response = cb(internal.server_ptr, &goal_handle, internal.c_context);
    // C: REJECT=0, ACCEPT=1 ; Rust: Ok=0 (accepted), Rejected=1
    match c_response {
        nros_cancel_response_t::NROS_CANCEL_ACCEPT => nros_node::CancelResponse::Ok,
        nros_cancel_response_t::NROS_CANCEL_REJECT => nros_node::CancelResponse::Rejected,
    }
}

/// Get the `ActionServerInternal` from a server's inline `_internal` storage.
///
/// # Safety
/// The server must have been registered with the executor (state = INITIALIZED
/// and `_internal` written via `ptr::write`).
unsafe fn get_internal(server: *const nros_action_server_t) -> &'static ActionServerInternal {
    &(*server)._internal
}

// ============================================================================
// Action Server Functions
// ============================================================================

/// Get a zero-initialized action server.
///
/// `improper_ctypes_definitions` is silenced because
/// `ActionServerRawHandle` (transitively in `_internal.handle`) has a
/// function-pointer field whose *parameter* signature includes
/// `&mut dyn FnMut(...)`. Function pointers are FFI-safe themselves;
/// only invoking the field with a Rust trait object is non-FFI, and
/// no C caller does that.
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn nros_action_server_get_zero_initialized() -> nros_action_server_t {
    nros_action_server_t::default()
}

/// Initialize an action server.
///
/// Stores metadata (name, type, callbacks). RMW entity creation is deferred
/// to `nros_executor_add_action_server()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_server_init(
    server: *mut nros_action_server_t,
    node: *const nros_node_t,
    action_name: *const core::ffi::c_char,
    type_info: *const nros_action_type_t,
    goal_callback: nros_goal_callback_t,
    cancel_callback: nros_cancel_callback_t,
    accepted_callback: nros_accepted_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(server, node, action_name, type_info);

    if goal_callback.is_none() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        server,
        nros_action_server_state_t::NROS_ACTION_SERVER_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy action name (required — empty rejected)
    server.action_name_len = crate::util::copy_cstr_into(action_name, &mut server.action_name);
    if server.action_name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    server.type_name_len = crate::util::copy_cstr_into(type_info.type_name, &mut server.type_name);
    server.type_hash_len = crate::util::copy_cstr_into(type_info.type_hash, &mut server.type_hash);

    // Store callbacks and context
    server.goal_callback = goal_callback;
    server.cancel_callback = cancel_callback;
    server.accepted_callback = accepted_callback;
    server.context = context;
    server.node = node;

    // RMW entity creation is deferred to nros_executor_add_action_server()
    server._internal = ActionServerInternal::invalid_default();
    server.state = nros_action_server_state_t::NROS_ACTION_SERVER_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Publish feedback for an active goal.
///
/// Delegates to `ActionServerRawHandle::publish_feedback_raw`; the arena
/// enforces goal liveness and is the sole source of truth for lifecycle
/// state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_publish_feedback(
    server: *mut nros_action_server_t,
    goal: *const nros_goal_handle_t,
    feedback: *const u8,
    feedback_len: usize,
) -> nros_ret_t {
    validate_not_null!(server, goal, feedback);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return NROS_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalId {
        uuid: (*goal).uuid.uuid,
    };
    let data = core::slice::from_raw_parts(feedback, feedback_len);

    // C serialize produces [CDR_HEADER][fields], but publish_feedback_raw
    // expects raw fields only (it adds its own CDR header + GoalId framing).
    let fields = strip_cdr_header(data);

    match handle.publish_feedback_raw(executor, &goal_id, fields) {
        Ok(()) => NROS_RET_OK,
        Err(_) => NROS_RET_ERROR,
    }
}

/// Mark a goal as succeeded with a result.
///
/// Delegates to `ActionServerRawHandle::complete_goal_raw`; the arena
/// owns the active-goals vector and retires the goal there.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_succeed(
    server: *mut nros_action_server_t,
    goal: *const nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return NROS_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalId {
        uuid: (*goal).uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER][fields], but complete_goal_raw
    // expects raw fields only (it stores them in the slab and adds its own
    // CDR header when replying to get_result requests).
    let result_fields = strip_cdr_header(result_data);

    handle.complete_goal_raw(
        executor,
        &goal_id,
        nros_node::GoalStatus::Succeeded,
        result_fields,
    );

    NROS_RET_OK
}

/// Mark a goal as aborted with an optional result.
///
/// Delegates to `ActionServerRawHandle::complete_goal_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_abort(
    server: *mut nros_action_server_t,
    goal: *const nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return NROS_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalId {
        uuid: (*goal).uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER][fields] — strip the header.
    let result_fields = strip_cdr_header(result_data);

    handle.complete_goal_raw(
        executor,
        &goal_id,
        nros_node::GoalStatus::Aborted,
        result_fields,
    );

    NROS_RET_OK
}

/// Mark a goal as canceled with an optional result.
///
/// Delegates to `ActionServerRawHandle::complete_goal_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_canceled(
    server: *mut nros_action_server_t,
    goal: *const nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return NROS_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalId {
        uuid: (*goal).uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER][fields] — strip the header.
    let result_fields = strip_cdr_header(result_data);

    handle.complete_goal_raw(
        executor,
        &goal_id,
        nros_node::GoalStatus::Canceled,
        result_fields,
    );

    NROS_RET_OK
}

/// Execute a goal (transition to `Executing`).
///
/// Idempotent: delegates to `ActionServerRawHandle::set_goal_status` which
/// is a no-op if the goal isn't in the active-goals vector. Returns
/// `NROS_RET_OK` on success, `NROS_RET_NOT_INIT` if the server isn't
/// registered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_execute(
    server: *mut nros_action_server_t,
    goal: *const nros_goal_handle_t,
) -> nros_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return NROS_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalId {
        uuid: (*goal).uuid.uuid,
    };
    handle.set_goal_status(executor, &goal_id, nros_node::GoalStatus::Executing);
    NROS_RET_OK
}

/// Get the number of currently active goals.
///
/// Reads from the arena via `ActionServerRawHandle::active_goal_count`.
/// Returns `0` if the server isn't registered or has been finalised.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_server_get_active_goal_count(
    server: *const nros_action_server_t,
) -> usize {
    if server.is_null() {
        return 0;
    }
    let server_ref = &*server;
    if server_ref.state != nros_action_server_state_t::NROS_ACTION_SERVER_STATE_INITIALIZED {
        return 0;
    }
    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return 0;
    }
    let handle = internal.handle;
    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    handle.active_goal_count(executor)
}

/// Look up a goal's current status in the arena by UUID.
///
/// Returns `NROS_RET_OK` and writes the arena-sourced status on success.
/// Returns `NROS_RET_NOT_FOUND` if the arena has already retired the goal
/// (completed + result delivered, or cancelled + acknowledged).
///
/// This is the authoritative successor to the removed `goal->status`
/// field: lifecycle transitions are driven by
/// `ActionServerArenaEntry::active_goals` and read back through this
/// function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_get_goal_status(
    server: *const nros_action_server_t,
    goal: *const nros_goal_handle_t,
    status: *mut nros_goal_status_t,
) -> nros_ret_t {
    validate_not_null!(server, goal, status);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return NROS_RET_NOT_INIT;
    }
    let handle = internal.handle;
    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalId {
        uuid: (*goal).uuid.uuid,
    };
    match handle.goal_status(executor, &goal_id) {
        Some(s) => {
            *status = goal_status_from_core(s);
            NROS_RET_OK
        }
        None => NROS_RET_NOT_FOUND,
    }
}

/// Map a `nros_node::GoalStatus` to its `nros_goal_status_t` equivalent.
fn goal_status_from_core(status: nros_node::GoalStatus) -> nros_goal_status_t {
    use nros_node::GoalStatus;
    match status {
        GoalStatus::Unknown => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
        GoalStatus::Accepted => nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED,
        GoalStatus::Executing => nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING,
        GoalStatus::Canceling => nros_goal_status_t::NROS_GOAL_STATUS_CANCELING,
        GoalStatus::Succeeded => nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
        GoalStatus::Canceled => nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
        GoalStatus::Aborted => nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
    }
}

/// Finalize an action server.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_server_fini(server: *mut nros_action_server_t) -> nros_ret_t {
    validate_not_null!(server);

    let server = &mut *server;

    validate_state!(
        server,
        nros_action_server_state_t::NROS_ACTION_SERVER_STATE_INITIALIZED
    );

    // Reset the internal back to its sentinel (no Drop impl needed —
    // the arena owns the actual action server).
    server._internal = ActionServerInternal::invalid_default();

    server.goal_callback = None;
    server.cancel_callback = None;
    server.accepted_callback = None;
    server.context = ptr::null_mut();
    server.node = ptr::null();
    server.state = nros_action_server_state_t::NROS_ACTION_SERVER_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Kani Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;
    use core::ptr;

    // Helper to create a dummy action type info
    fn dummy_action_type() -> nros_action_type_t {
        let type_name = b"example_interfaces::action::dds_::Fibonacci_\0";
        let type_hash = b"RIHS01_test\0";
        nros_action_type_t {
            type_name: type_name.as_ptr() as *const core::ffi::c_char,
            type_hash: type_hash.as_ptr() as *const core::ffi::c_char,
            goal_serialized_size_max: 8,
            result_serialized_size_max: 264,
            feedback_serialized_size_max: 264,
        }
    }

    // Helper goal callback
    unsafe extern "C" fn dummy_goal_callback(
        _server: *mut nros_action_server_t,
        _goal: *const nros_goal_handle_t,
        _req: *const u8,
        _len: usize,
        _ctx: *mut core::ffi::c_void,
    ) -> nros_goal_response_t {
        nros_goal_response_t::NROS_GOAL_ACCEPT_AND_EXECUTE
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_init_null_ptrs() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::nros_node_get_zero_initialized();

        // NULL server
        assert_eq!(
            unsafe {
                nros_action_server_init(
                    ptr::null_mut(),
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                    Some(dummy_goal_callback),
                    None,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL node
        let mut srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_server_init(
                    &mut srv,
                    ptr::null(),
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                    Some(dummy_goal_callback),
                    None,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL action_name
        let mut srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_server_init(
                    &mut srv,
                    &node,
                    ptr::null(),
                    &type_info,
                    Some(dummy_goal_callback),
                    None,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL type_info
        let mut srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_server_init(
                    &mut srv,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    ptr::null(),
                    Some(dummy_goal_callback),
                    None,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_init_none_goal_callback() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::nros_node_get_zero_initialized();

        let mut srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_server_init(
                    &mut srv,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                    None, // goal_callback is required
                    None,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_init_uninit_node() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::nros_node_get_zero_initialized();

        let mut srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_server_init(
                    &mut srv,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                    Some(dummy_goal_callback),
                    None,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_zero_initialized_state() {
        let srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            srv.state,
            nros_action_server_state_t::NROS_ACTION_SERVER_STATE_UNINITIALIZED,
        );
        assert!(srv._internal.handle.is_invalid());
        assert!(srv._internal.executor_ptr.is_null());
        assert!(srv.node.is_null());
        assert!(srv.goal_callback.is_none());
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_fini_null_safety() {
        // NULL → INVALID_ARGUMENT
        assert_eq!(
            unsafe { nros_action_server_fini(ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // UNINITIALIZED → NOT_INIT
        let mut srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_server_fini(&mut srv) },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_double_init_rejected() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let mut node = crate::node::nros_node_get_zero_initialized();
        node.state = crate::node::nros_node_state_t::NROS_NODE_STATE_INITIALIZED;

        let mut srv = nros_action_server_get_zero_initialized();
        let ret = unsafe {
            nros_action_server_init(
                &mut srv,
                &node,
                action_name.as_ptr() as *const core::ffi::c_char,
                &type_info,
                Some(dummy_goal_callback),
                None,
                None,
                ptr::null_mut(),
            )
        };
        assert_eq!(ret, NROS_RET_OK);

        // Second init → BAD_SEQUENCE
        assert_eq!(
            unsafe {
                nros_action_server_init(
                    &mut srv,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                    Some(dummy_goal_callback),
                    None,
                    None,
                    ptr::null_mut(),
                )
            },
            NROS_RET_BAD_SEQUENCE,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_active_goal_count_null() {
        let count = unsafe { nros_action_server_get_active_goal_count(ptr::null()) };
        assert_eq!(count, 0);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_publish_feedback_null_ptrs() {
        let feedback = [0u8; 8];
        let goal = nros_goal_handle_t::default();

        // NULL server
        assert_eq!(
            unsafe { nros_action_publish_feedback(ptr::null_mut(), &goal, feedback.as_ptr(), 8) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL goal
        let mut srv = nros_action_server_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_publish_feedback(&mut srv, ptr::null(), feedback.as_ptr(), 8) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL feedback
        assert_eq!(
            unsafe { nros_action_publish_feedback(&mut srv, &goal, ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_succeed_null_ptr() {
        assert_eq!(
            unsafe { nros_action_succeed(ptr::null_mut(), ptr::null(), ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_abort_null_ptr() {
        assert_eq!(
            unsafe { nros_action_abort(ptr::null_mut(), ptr::null(), ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_canceled_null_ptr() {
        assert_eq!(
            unsafe { nros_action_canceled(ptr::null_mut(), ptr::null(), ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }
}
