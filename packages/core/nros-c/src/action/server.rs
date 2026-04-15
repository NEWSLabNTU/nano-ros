//! Action server implementation.

use core::ffi::c_void;
use core::ptr;

use super::common::*;
use crate::config::ACTION_SERVER_INTERNAL_OPAQUE_U64S;
use crate::constants::{
    MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN, NROS_MAX_CONCURRENT_GOALS,
};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};

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
    /// Goal handles
    pub goals: [nros_goal_handle_t; NROS_MAX_CONCURRENT_GOALS],
    /// Number of active goals
    pub active_goal_count: usize,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Opaque inline storage for internal implementation
    pub _internal: [u64; ACTION_SERVER_INTERNAL_OPAQUE_U64S],
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
            goals: [
                nros_goal_handle_t::default(),
                nros_goal_handle_t::default(),
                nros_goal_handle_t::default(),
                nros_goal_handle_t::default(),
            ],
            active_goal_count: 0,
            node: ptr::null(),
            _internal: [0u64; ACTION_SERVER_INTERNAL_OPAQUE_U64S],
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
pub(crate) struct ActionServerInternal {
    /// Handle returned by executor registration.
    /// `None` until registration completes (set immediately after).
    pub(crate) handle: Option<nros_node::ActionServerRawHandle>,
    /// Pointer to the internal Rust executor (`CExecutor`).
    pub(crate) executor_ptr: *mut c_void,
    /// C goal callback from init.
    pub(crate) c_goal_callback: unsafe extern "C" fn(
        *const nros_goal_uuid_t,
        *const u8,
        usize,
        *mut c_void,
    ) -> nros_goal_response_t,
    /// C cancel callback from init (may be None).
    pub(crate) c_cancel_callback: nros_cancel_callback_t,
    /// C accepted callback from init (may be None).
    pub(crate) c_accepted_callback: nros_accepted_callback_t,
    /// C user context from init.
    pub(crate) c_context: *mut c_void,
    /// Pointer back to the C action server struct.
    pub(crate) server_ptr: *mut nros_action_server_t,
}

/// Goal callback trampoline for nros-node.
///
/// Wraps the C `nros_goal_callback_t` as a `RawGoalCallback`. On acceptance,
/// fills a C-side goal slot and calls the C accepted callback.
pub(crate) unsafe extern "C" fn goal_callback_trampoline(
    goal_id: *const nros_core::GoalId,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut c_void,
) -> nros_core::GoalResponse {
    let internal = &*(context as *const ActionServerInternal);
    let server = &mut *internal.server_ptr;

    // GoalId and nros_goal_uuid_t are layout-compatible (both [u8; 16])
    let uuid_ptr = goal_id as *const nros_goal_uuid_t;

    // goal_data contains the full CDR request: [CDR_HDR(4)][GoalId(20)][goal_fields].
    // The C callback expects CDR-encoded goal data: [CDR_HDR(4)][goal_fields].
    // Extract goal fields (after CDR header + GoalId) and prepend a CDR header.
    let goal_framing = 24usize; // CDR header (4) + GoalId seq_len (4) + UUID (16)
    let goal_slice = core::slice::from_raw_parts(goal_data, goal_len);

    // Build [CDR_HEADER][goal_fields] on the stack (must outlive the callback)
    let mut cb_buf = [0u8; 512];
    let (cb_ptr, cb_len) = if goal_len > goal_framing {
        let fields = &goal_slice[goal_framing..];
        cb_buf[0] = 0x00;
        cb_buf[1] = 0x01;
        cb_buf[2] = 0x00;
        cb_buf[3] = 0x00;
        let copy_len = fields.len().min(cb_buf.len() - 4);
        cb_buf[4..4 + copy_len].copy_from_slice(&fields[..copy_len]);
        (cb_buf.as_ptr(), 4 + copy_len)
    } else {
        (goal_data, goal_len)
    };

    // Call the C goal callback with CDR-encoded goal data
    let c_response = (internal.c_goal_callback)(uuid_ptr, cb_ptr, cb_len, internal.c_context);

    // Map C response to Rust GoalResponse
    let response = match c_response {
        nros_goal_response_t::NROS_GOAL_REJECT => return nros_core::GoalResponse::Reject,
        nros_goal_response_t::NROS_GOAL_ACCEPT_AND_EXECUTE => {
            nros_core::GoalResponse::AcceptAndExecute
        }
        nros_goal_response_t::NROS_GOAL_ACCEPT_AND_DEFER => nros_core::GoalResponse::AcceptAndDefer,
    };

    // Goal was accepted — fill a C-side goal slot. The user's
    // `c_accepted_callback` is NOT invoked here: it runs from
    // `accepted_callback_trampoline` after the arena layer has sent the
    // accept reply to the client. Running it inline would delay the reply
    // until the long-running execution finished, causing the client to
    // time out waiting for acceptance.
    if let Some(slot) = server.goals.iter_mut().find(|g| !g.active) {
        slot.uuid = *uuid_ptr;
        slot.status = match response {
            nros_core::GoalResponse::AcceptAndExecute => {
                nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING
            }
            nros_core::GoalResponse::AcceptAndDefer => {
                nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED
            }
            _ => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
        };
        slot.active = true;
        slot.server = internal.server_ptr;
        server.active_goal_count += 1;
    }

    response
}

/// Post-accept trampoline for nros-node.
///
/// Invoked by the arena *after* `ActionServerCore::accept_goal` has sent
/// the accept reply to the client. Finds the C-side goal slot matching
/// `goal_id` and calls the user-supplied `c_accepted_callback`.
pub(crate) unsafe extern "C" fn accepted_callback_trampoline(
    goal_id: *const nros_core::GoalId,
    context: *mut c_void,
) {
    let internal = &*(context as *const ActionServerInternal);
    let server = &mut *internal.server_ptr;
    let uuid = (*goal_id).uuid;

    let Some(slot) = server.goals.iter_mut().find(|g| g.active && g.uuid.uuid == uuid) else {
        return;
    };

    if let Some(cb) = internal.c_accepted_callback {
        cb(slot as *mut _, internal.c_context);
    }
}

/// Cancel callback trampoline for nros-node.
///
/// The Rust `RawCancelCallback` receives `(goal_id, status, context)`,
/// while the C `nros_cancel_callback_t` receives `(goal_handle, context)`.
/// This trampoline finds the matching C-side goal slot, updates its status
/// to CANCELING, and calls the C cancel callback.
pub(crate) unsafe extern "C" fn cancel_callback_trampoline(
    goal_id: *const nros_core::GoalId,
    _status: nros_core::GoalStatus,
    context: *mut c_void,
) -> nros_core::CancelResponse {
    let internal = &*(context as *const ActionServerInternal);
    let server = &mut *internal.server_ptr;

    // Find the C-side goal matching this goal_id
    let goal_id_ref = &*goal_id;
    let goal_slot = server
        .goals
        .iter_mut()
        .find(|g| g.active && g.uuid.uuid == goal_id_ref.uuid);

    match (internal.c_cancel_callback, goal_slot) {
        (Some(cb), Some(goal)) => {
            // Update goal status to canceling before calling the callback
            goal.status = nros_goal_status_t::NROS_GOAL_STATUS_CANCELING;

            let c_response = cb(goal as *mut _, internal.c_context);

            // Map C cancel response to Rust CancelResponse
            // C: REJECT=0, ACCEPT=1
            // Rust: Ok=0 (accepted), Rejected=1
            match c_response {
                nros_cancel_response_t::NROS_CANCEL_ACCEPT => nros_core::CancelResponse::Ok,
                nros_cancel_response_t::NROS_CANCEL_REJECT => {
                    // Revert status since cancel was rejected
                    goal.status = nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING;
                    nros_core::CancelResponse::Rejected
                }
            }
        }
        (None, _) => {
            // No cancel callback — accept by default
            nros_core::CancelResponse::Ok
        }
        (_, None) => {
            // Goal not found in C-side tracking
            nros_core::CancelResponse::UnknownGoal
        }
    }
}

/// Get the `ActionServerInternal` from a server's inline `_internal` storage.
///
/// # Safety
/// The server must have been registered with the executor (state = INITIALIZED
/// and `_internal` written via `ptr::write`).
unsafe fn get_internal(server: *const nros_action_server_t) -> &'static ActionServerInternal {
    &*((*server)._internal.as_ptr() as *const ActionServerInternal)
}

// ============================================================================
// Action Server Functions
// ============================================================================

/// Get a zero-initialized action server.
#[unsafe(no_mangle)]
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

    // Copy action name
    let name_ptr = action_name as *const u8;
    let mut len = 0usize;
    while len < MAX_ACTION_NAME_LEN - 1 {
        let c = *name_ptr.add(len);
        if c == 0 {
            break;
        }
        server.action_name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }
    server.action_name[len] = 0;
    server.action_name_len = len;

    // Copy type name
    if !type_info.type_name.is_null() {
        let type_ptr = type_info.type_name as *const u8;
        len = 0;
        while len < MAX_TYPE_NAME_LEN - 1 {
            let c = *type_ptr.add(len);
            if c == 0 {
                break;
            }
            server.type_name[len] = c;
            len += 1;
        }
        server.type_name[len] = 0;
        server.type_name_len = len;
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
            server.type_hash[len] = c;
            len += 1;
        }
        server.type_hash[len] = 0;
        server.type_hash_len = len;
    }

    // Store callbacks and context
    server.goal_callback = goal_callback;
    server.cancel_callback = cancel_callback;
    server.accepted_callback = accepted_callback;
    server.context = context;
    server.node = node;
    server.active_goal_count = 0;

    // Initialize goal handles with pointer back to server
    let server_ptr = server as *mut nros_action_server_t;
    for goal in server.goals.iter_mut() {
        *goal = nros_goal_handle_t::default();
        goal.server = server_ptr;
    }

    // RMW entity creation is deferred to nros_executor_add_action_server()
    server._internal = [0u64; ACTION_SERVER_INTERNAL_OPAQUE_U64S];
    server.state = nros_action_server_state_t::NROS_ACTION_SERVER_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Publish feedback for an executing goal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_publish_feedback(
    goal: *mut nros_goal_handle_t,
    feedback: *const u8,
    feedback_len: usize,
) -> nros_ret_t {
    validate_not_null!(goal, feedback);

    let goal = &*goal;

    // Check goal is in executing state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    if goal.server.is_null() {
        return NROS_RET_NOT_INIT;
    }
    let internal = get_internal(goal.server);
    let handle = match internal.handle {
        Some(h) => h,
        None => return NROS_RET_NOT_INIT,
    };

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_core::GoalId {
        uuid: goal.uuid.uuid,
    };
    let data = core::slice::from_raw_parts(feedback, feedback_len);

    // C serialize produces [CDR_HEADER(4)][fields], but publish_feedback_raw
    // expects raw fields only (it adds its own CDR header + GoalId framing).
    let fields = if data.len() > 4 { &data[4..] } else { data };

    match handle.publish_feedback_raw(executor, &goal_id, fields) {
        Ok(()) => NROS_RET_OK,
        Err(_) => NROS_RET_ERROR,
    }
}

/// Mark a goal as succeeded with a result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_succeed(
    goal: *mut nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    validate_not_null!(goal);

    let goal = &mut *goal;

    // Check goal is in executing state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    if goal.server.is_null() {
        return NROS_RET_NOT_INIT;
    }
    let internal = get_internal(goal.server);
    let handle = match internal.handle {
        Some(h) => h,
        None => return NROS_RET_NOT_INIT,
    };

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_core::GoalId {
        uuid: goal.uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER(4)][fields], but complete_goal_raw
    // expects raw fields only (it stores them in the slab and adds its own
    // CDR header when replying to get_result requests).
    let result_fields = if result_data.len() > 4 {
        &result_data[4..]
    } else {
        result_data
    };

    // Delegate to nros-node executor
    handle.complete_goal_raw(
        executor,
        &goal_id,
        nros_core::GoalStatus::Succeeded,
        result_fields,
    );

    // Update C-side goal state
    goal.status = nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED;
    goal.active = false;
    let server = &mut *goal.server;
    if server.active_goal_count > 0 {
        server.active_goal_count -= 1;
    }

    NROS_RET_OK
}

/// Mark a goal as aborted with an optional result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_abort(
    goal: *mut nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    validate_not_null!(goal);

    let goal = &mut *goal;

    // Check goal is in executing or canceling state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING
        && goal.status != nros_goal_status_t::NROS_GOAL_STATUS_CANCELING
    {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    if goal.server.is_null() {
        return NROS_RET_NOT_INIT;
    }
    let internal = get_internal(goal.server);
    let handle = match internal.handle {
        Some(h) => h,
        None => return NROS_RET_NOT_INIT,
    };

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_core::GoalId {
        uuid: goal.uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER(4)][fields] — strip the header.
    let result_fields = if result_data.len() > 4 {
        &result_data[4..]
    } else {
        result_data
    };

    handle.complete_goal_raw(
        executor,
        &goal_id,
        nros_core::GoalStatus::Aborted,
        result_fields,
    );

    goal.status = nros_goal_status_t::NROS_GOAL_STATUS_ABORTED;
    goal.active = false;
    let server = &mut *goal.server;
    if server.active_goal_count > 0 {
        server.active_goal_count -= 1;
    }

    NROS_RET_OK
}

/// Mark a goal as canceled with an optional result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_canceled(
    goal: *mut nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    validate_not_null!(goal);

    let goal = &mut *goal;

    // Check goal is in canceling state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_CANCELING {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    if goal.server.is_null() {
        return NROS_RET_NOT_INIT;
    }
    let internal = get_internal(goal.server);
    let handle = match internal.handle {
        Some(h) => h,
        None => return NROS_RET_NOT_INIT,
    };

    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_core::GoalId {
        uuid: goal.uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER(4)][fields] — strip the header.
    let result_fields = if result_data.len() > 4 {
        &result_data[4..]
    } else {
        result_data
    };

    handle.complete_goal_raw(
        executor,
        &goal_id,
        nros_core::GoalStatus::Canceled,
        result_fields,
    );

    goal.status = nros_goal_status_t::NROS_GOAL_STATUS_CANCELED;
    goal.active = false;
    let server = &mut *goal.server;
    if server.active_goal_count > 0 {
        server.active_goal_count -= 1;
    }

    NROS_RET_OK
}

/// Execute a goal (transition from accepted to executing).
///
/// Idempotent: if the goal was accepted with `ACCEPT_AND_EXECUTE` the
/// trampoline already set the status to `EXECUTING`, so calling this is a
/// no-op and returns `NROS_RET_OK`. Only returns `NROS_RET_NOT_ALLOWED` if
/// the goal is in a terminal state (succeeded/canceled/aborted/unknown).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_execute(goal: *mut nros_goal_handle_t) -> nros_ret_t {
    validate_not_null!(goal);

    let goal = &mut *goal;

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    // Already executing — nothing to do.
    if goal.status == nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING {
        return NROS_RET_OK;
    }

    // Must be in ACCEPTED (deferred) state to transition.
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED {
        return NROS_RET_NOT_ALLOWED;
    }

    // Update nros-node side if registered with executor
    if !goal.server.is_null() {
        let internal = get_internal(goal.server);
        if let Some(handle) = internal.handle {
            let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
            let goal_id = nros_core::GoalId {
                uuid: goal.uuid.uuid,
            };
            handle.set_goal_status(executor, &goal_id, nros_core::GoalStatus::Executing);
        }
    }

    goal.status = nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING;
    NROS_RET_OK
}

/// Get the number of active goals.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_server_get_active_goal_count(
    server: *const nros_action_server_t,
) -> usize {
    if server.is_null() {
        return 0;
    }

    let server = &*server;
    server.active_goal_count
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

    // Drop the internal implementation in place
    core::ptr::drop_in_place(server._internal.as_mut_ptr() as *mut ActionServerInternal);
    server._internal = [0u64; ACTION_SERVER_INTERNAL_OPAQUE_U64S];

    // Reset all goal handles
    for goal in server.goals.iter_mut() {
        *goal = nros_goal_handle_t::default();
    }

    server.goal_callback = None;
    server.cancel_callback = None;
    server.accepted_callback = None;
    server.context = ptr::null_mut();
    server.node = ptr::null();
    server.active_goal_count = 0;
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
        _uuid: *const nros_goal_uuid_t,
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
        assert_eq!(srv._internal, [0u64; ACTION_SERVER_INTERNAL_OPAQUE_U64S]);
        assert!(srv.node.is_null());
        assert_eq!(srv.active_goal_count, 0);
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

        // NULL goal
        assert_eq!(
            unsafe { nros_action_publish_feedback(ptr::null_mut(), feedback.as_ptr(), 8) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL feedback
        let mut goal = nros_goal_handle_t::default();
        assert_eq!(
            unsafe { nros_action_publish_feedback(&mut goal, ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_succeed_null_ptr() {
        assert_eq!(
            unsafe { nros_action_succeed(ptr::null_mut(), ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_abort_null_ptr() {
        assert_eq!(
            unsafe { nros_action_abort(ptr::null_mut(), ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_canceled_null_ptr() {
        assert_eq!(
            unsafe { nros_action_canceled(ptr::null_mut(), ptr::null(), 0) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }
}
