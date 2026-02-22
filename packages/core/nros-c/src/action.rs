//! Action API for nros C API.
//!
//! Actions provide long-running task execution with feedback and cancellation.
//! This module implements both action servers and clients.
//!
//! The action server follows the same metadata-only init → executor registration
//! pattern as subscriptions and services:
//! 1. `nros_action_server_init()` stores metadata (name, type, callbacks)
//! 2. `nros_executor_add_action_server()` creates RMW entities and registers
//!    with the nros-node executor
//! 3. Operation functions delegate through `ActionServerRawHandle`

use core::ffi::{c_char, c_void};
use core::ptr;

use crate::constants::{MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};

// ============================================================================
// Constants
// ============================================================================

use crate::constants::NROS_MAX_CONCURRENT_GOALS;

// ============================================================================
// Goal Status
// ============================================================================

/// Goal status enumeration.
///
/// Compatible with action_msgs/msg/GoalStatus values.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_goal_status_t {
    /// Goal state is unknown
    NROS_GOAL_STATUS_UNKNOWN = 0,
    /// Goal was accepted and is pending execution
    NROS_GOAL_STATUS_ACCEPTED = 1,
    /// Goal is currently being executed
    NROS_GOAL_STATUS_EXECUTING = 2,
    /// Goal is being canceled
    NROS_GOAL_STATUS_CANCELING = 3,
    /// Goal completed successfully
    NROS_GOAL_STATUS_SUCCEEDED = 4,
    /// Goal was canceled
    NROS_GOAL_STATUS_CANCELED = 5,
    /// Goal was aborted (failed)
    NROS_GOAL_STATUS_ABORTED = 6,
}

/// Goal response codes for goal request handling.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_goal_response_t {
    /// Reject the goal
    NROS_GOAL_REJECT = 0,
    /// Accept the goal and start executing immediately
    NROS_GOAL_ACCEPT_AND_EXECUTE = 1,
    /// Accept the goal but defer execution
    NROS_GOAL_ACCEPT_AND_DEFER = 2,
}

/// Cancel response codes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cancel_response_t {
    /// Reject the cancel request
    NROS_CANCEL_REJECT = 0,
    /// Accept the cancel request
    NROS_CANCEL_ACCEPT = 1,
}

// ============================================================================
// Action Type Info
// ============================================================================

/// Action type information.
#[repr(C)]
pub struct nros_action_type_t {
    /// Action type name (e.g., "example_interfaces::action::Fibonacci")
    pub type_name: *const c_char,
    /// Action type hash
    pub type_hash: *const c_char,
    /// Maximum serialized size of goal message
    pub goal_serialized_size_max: usize,
    /// Maximum serialized size of result message
    pub result_serialized_size_max: usize,
    /// Maximum serialized size of feedback message
    pub feedback_serialized_size_max: usize,
}

// ============================================================================
// Goal UUID
// ============================================================================

/// Goal UUID structure (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct nros_goal_uuid_t {
    /// UUID bytes
    pub uuid: [u8; 16],
}

// ============================================================================
// Goal Handle
// ============================================================================

/// Goal handle structure.
#[repr(C)]
pub struct nros_goal_handle_t {
    /// Goal UUID
    pub uuid: nros_goal_uuid_t,
    /// Current status
    pub status: nros_goal_status_t,
    /// Whether this goal slot is in use
    pub active: bool,
    /// User context pointer for this goal
    pub context: *mut c_void,
    /// Pointer back to the action server (internal)
    pub server: *mut nros_action_server_t,
}

impl Default for nros_goal_handle_t {
    fn default() -> Self {
        Self {
            uuid: nros_goal_uuid_t::default(),
            status: nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
            active: false,
            context: ptr::null_mut(),
            server: ptr::null_mut(),
        }
    }
}

// ============================================================================
// Callback Types
// ============================================================================

/// Goal request callback type.
pub type nros_goal_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const nros_goal_uuid_t,
        goal_request: *const u8,
        goal_len: usize,
        context: *mut c_void,
    ) -> nros_goal_response_t,
>;

/// Cancel request callback type.
pub type nros_cancel_callback_t = Option<
    unsafe extern "C" fn(
        goal: *mut nros_goal_handle_t,
        context: *mut c_void,
    ) -> nros_cancel_response_t,
>;

/// Goal accepted callback type.
pub type nros_accepted_callback_t =
    Option<unsafe extern "C" fn(goal: *mut nros_goal_handle_t, context: *mut c_void)>;

/// Feedback callback type (for client).
pub type nros_feedback_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const nros_goal_uuid_t,
        feedback: *const u8,
        feedback_len: usize,
        context: *mut c_void,
    ),
>;

/// Result callback type (for client).
pub type nros_result_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const nros_goal_uuid_t,
        status: nros_goal_status_t,
        result: *const u8,
        result_len: usize,
        context: *mut c_void,
    ),
>;

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
    /// Opaque pointer to internal implementation (`Box<ActionServerInternal>`)
    pub _internal: *mut c_void,
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
            _internal: ptr::null_mut(),
        }
    }
}

// ============================================================================
// Internal implementation (delegates to nros-node executor)
// ============================================================================

/// Internal state for the action client.
///
/// Holds the `ActionClientCore` created during `nros_action_client_init()`.
/// The core contains 3 service clients (send_goal, cancel_goal, get_result)
/// and 1 feedback subscriber.
#[cfg(feature = "alloc")]
struct ActionClientInternal {
    core: nros_node::ActionClientCore<
        nros::internals::RmwServiceClient,
        nros::internals::RmwSubscriber,
        { crate::executor::MESSAGE_BUFFER_SIZE },
        { crate::executor::MESSAGE_BUFFER_SIZE },
        { crate::executor::MESSAGE_BUFFER_SIZE },
    >,
}

/// Internal state created during executor registration.
///
/// Holds the `ActionServerRawHandle` and C callback pointers needed by
/// the goal/cancel trampolines.
#[cfg(feature = "alloc")]
pub(crate) struct ActionServerInternal {
    /// Handle returned by `Executor::add_action_server_raw()`.
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
#[cfg(feature = "alloc")]
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

    // Call the C goal callback
    let c_response = (internal.c_goal_callback)(uuid_ptr, goal_data, goal_len, internal.c_context);

    // Map C response to Rust GoalResponse
    let response = match c_response {
        nros_goal_response_t::NROS_GOAL_REJECT => return nros_core::GoalResponse::Reject,
        nros_goal_response_t::NROS_GOAL_ACCEPT_AND_EXECUTE => {
            nros_core::GoalResponse::AcceptAndExecute
        }
        nros_goal_response_t::NROS_GOAL_ACCEPT_AND_DEFER => nros_core::GoalResponse::AcceptAndDefer,
    };

    // Goal was accepted — fill a C-side goal slot
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

        // Call the accepted callback
        if let Some(cb) = internal.c_accepted_callback {
            cb(slot as *mut _, internal.c_context);
        }
    }

    response
}

/// Cancel callback trampoline for nros-node.
///
/// The Rust `RawCancelCallback` receives `(goal_id, status, context)`,
/// while the C `nros_cancel_callback_t` receives `(goal_handle, context)`.
/// This trampoline finds the matching C-side goal slot, updates its status
/// to CANCELING, and calls the C cancel callback.
#[cfg(feature = "alloc")]
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

/// Get the `ActionServerInternal` from a server's `_internal` pointer.
#[cfg(feature = "alloc")]
unsafe fn get_internal(
    server: *const nros_action_server_t,
) -> Option<&'static ActionServerInternal> {
    let ptr = (*server)._internal;
    if ptr.is_null() {
        None
    } else {
        Some(&*(ptr as *const ActionServerInternal))
    }
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
    action_name: *const c_char,
    type_info: *const nros_action_type_t,
    goal_callback: nros_goal_callback_t,
    cancel_callback: nros_cancel_callback_t,
    accepted_callback: nros_accepted_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    // Validate required arguments
    if server.is_null() || node.is_null() || action_name.is_null() || type_info.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    if goal_callback.is_none() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;
    let node_ref = &*node;
    let type_info = &*type_info;

    // Check if server is already initialized
    if server.state != nros_action_server_state_t::NROS_ACTION_SERVER_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Check if node is initialized
    if node_ref.state != nros_node_state_t::NROS_NODE_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

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
    server._internal = ptr::null_mut();
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
    if goal.is_null() || feedback.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let goal = &*goal;

    // Check goal is in executing state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    #[cfg(feature = "alloc")]
    {
        if goal.server.is_null() {
            return NROS_RET_NOT_INIT;
        }
        let internal = match get_internal(goal.server) {
            Some(i) => i,
            None => return NROS_RET_NOT_INIT,
        };
        let handle = match internal.handle {
            Some(h) => h,
            None => return NROS_RET_NOT_INIT,
        };

        let executor = crate::executor::get_executor(internal.executor_ptr);
        let goal_id = nros_core::GoalId {
            uuid: goal.uuid.uuid,
        };
        let data = core::slice::from_raw_parts(feedback, feedback_len);

        match handle.publish_feedback_raw(executor, &goal_id, data) {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Mark a goal as succeeded with a result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_succeed(
    goal: *mut nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    if goal.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let goal = &mut *goal;

    // Check goal is in executing state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    #[cfg(feature = "alloc")]
    {
        if goal.server.is_null() {
            return NROS_RET_NOT_INIT;
        }
        let internal = match get_internal(goal.server) {
            Some(i) => i,
            None => return NROS_RET_NOT_INIT,
        };
        let handle = match internal.handle {
            Some(h) => h,
            None => return NROS_RET_NOT_INIT,
        };

        let executor = crate::executor::get_executor(internal.executor_ptr);
        let goal_id = nros_core::GoalId {
            uuid: goal.uuid.uuid,
        };
        let result_data = if !result.is_null() {
            core::slice::from_raw_parts(result, result_len)
        } else {
            &[]
        };

        // Delegate to nros-node executor
        handle.complete_goal_raw(
            executor,
            &goal_id,
            nros_core::GoalStatus::Succeeded,
            result_data,
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

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Mark a goal as aborted with an optional result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_abort(
    goal: *mut nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    if goal.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

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

    #[cfg(feature = "alloc")]
    {
        if goal.server.is_null() {
            return NROS_RET_NOT_INIT;
        }
        let internal = match get_internal(goal.server) {
            Some(i) => i,
            None => return NROS_RET_NOT_INIT,
        };
        let handle = match internal.handle {
            Some(h) => h,
            None => return NROS_RET_NOT_INIT,
        };

        let executor = crate::executor::get_executor(internal.executor_ptr);
        let goal_id = nros_core::GoalId {
            uuid: goal.uuid.uuid,
        };
        let result_data = if !result.is_null() {
            core::slice::from_raw_parts(result, result_len)
        } else {
            &[]
        };

        handle.complete_goal_raw(
            executor,
            &goal_id,
            nros_core::GoalStatus::Aborted,
            result_data,
        );

        goal.status = nros_goal_status_t::NROS_GOAL_STATUS_ABORTED;
        goal.active = false;
        let server = &mut *goal.server;
        if server.active_goal_count > 0 {
            server.active_goal_count -= 1;
        }

        NROS_RET_OK
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Mark a goal as canceled with an optional result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_canceled(
    goal: *mut nros_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> nros_ret_t {
    if goal.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let goal = &mut *goal;

    // Check goal is in canceling state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_CANCELING {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    #[cfg(feature = "alloc")]
    {
        if goal.server.is_null() {
            return NROS_RET_NOT_INIT;
        }
        let internal = match get_internal(goal.server) {
            Some(i) => i,
            None => return NROS_RET_NOT_INIT,
        };
        let handle = match internal.handle {
            Some(h) => h,
            None => return NROS_RET_NOT_INIT,
        };

        let executor = crate::executor::get_executor(internal.executor_ptr);
        let goal_id = nros_core::GoalId {
            uuid: goal.uuid.uuid,
        };
        let result_data = if !result.is_null() {
            core::slice::from_raw_parts(result, result_len)
        } else {
            &[]
        };

        handle.complete_goal_raw(
            executor,
            &goal_id,
            nros_core::GoalStatus::Canceled,
            result_data,
        );

        goal.status = nros_goal_status_t::NROS_GOAL_STATUS_CANCELED;
        goal.active = false;
        let server = &mut *goal.server;
        if server.active_goal_count > 0 {
            server.active_goal_count -= 1;
        }

        NROS_RET_OK
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Execute a goal (transition from accepted to executing).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_execute(goal: *mut nros_goal_handle_t) -> nros_ret_t {
    if goal.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let goal = &mut *goal;

    // Check goal is in accepted state
    if goal.status != nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED {
        return NROS_RET_NOT_ALLOWED;
    }

    if !goal.active {
        return NROS_RET_NOT_ALLOWED;
    }

    // Update nros-node side if registered with executor
    #[cfg(feature = "alloc")]
    if !goal.server.is_null()
        && let Some(internal) = get_internal(goal.server)
        && let Some(handle) = internal.handle
    {
        let executor = crate::executor::get_executor(internal.executor_ptr);
        let goal_id = nros_core::GoalId {
            uuid: goal.uuid.uuid,
        };
        handle.set_goal_status(executor, &goal_id, nros_core::GoalStatus::Executing);
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
    if server.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nros_action_server_state_t::NROS_ACTION_SERVER_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Drop the internal implementation
    #[cfg(feature = "alloc")]
    {
        if !server._internal.is_null() {
            let _internal =
                alloc::boxed::Box::from_raw(server._internal as *mut ActionServerInternal);
            // ActionServerInternal is dropped here
        }
    }

    server._internal = ptr::null_mut();

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
// Action Client
// ============================================================================

/// Action client state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_action_client_state_t {
    /// Not initialized
    NROS_ACTION_CLIENT_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_ACTION_CLIENT_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_ACTION_CLIENT_STATE_SHUTDOWN = 2,
}

/// Action client structure.
#[repr(C)]
pub struct nros_action_client_t {
    /// Current state
    pub state: nros_action_client_state_t,
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
    /// Feedback callback
    pub feedback_callback: nros_feedback_callback_t,
    /// Result callback
    pub result_callback: nros_result_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Opaque pointer to internal implementation
    pub _internal: *mut c_void,
}

impl Default for nros_action_client_t {
    fn default() -> Self {
        Self {
            state: nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_UNINITIALIZED,
            action_name: [0u8; MAX_ACTION_NAME_LEN],
            action_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            feedback_callback: None,
            result_callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ptr::null_mut(),
        }
    }
}

/// Get a zero-initialized action client.
#[unsafe(no_mangle)]
pub extern "C" fn nros_action_client_get_zero_initialized() -> nros_action_client_t {
    nros_action_client_t::default()
}

/// Initialize an action client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_init(
    client: *mut nros_action_client_t,
    node: *const nros_node_t,
    action_name: *const c_char,
    type_info: *const nros_action_type_t,
) -> nros_ret_t {
    // Validate required arguments
    if client.is_null() || node.is_null() || action_name.is_null() || type_info.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;
    let node_ref = &*node;
    let type_info = &*type_info;

    // Check if client is already initialized
    if client.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Check if node is initialized
    if node_ref.state != nros_node_state_t::NROS_NODE_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Copy action name
    let name_ptr = action_name as *const u8;
    let mut len = 0usize;
    while len < MAX_ACTION_NAME_LEN - 1 {
        let c = *name_ptr.add(len);
        if c == 0 {
            break;
        }
        client.action_name[len] = c;
        len += 1;
    }
    if len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }
    client.action_name[len] = 0;
    client.action_name_len = len;

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

    // Create the ActionClientCore with 3 service clients + 1 feedback subscriber.
    // This follows the service client init pattern (service.rs:590-634).
    #[cfg(feature = "alloc")]
    {
        use nros_rmw::{ActionInfo, QosSettings, ServiceInfo, Session, TopicInfo};

        // Get mutable support reference to access the session
        let support_mut = match node_ref.get_support_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };

        if support_mut.state != crate::support::nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED
        {
            return NROS_RET_NOT_INIT;
        }

        let domain_id = support_mut.domain_id as u32;

        let session = match support_mut.get_session_mut() {
            Some(s) => s,
            None => return NROS_RET_NOT_INIT,
        };

        let action_name_str =
            core::str::from_utf8_unchecked(&client.action_name[..client.action_name_len]);
        let type_str = core::str::from_utf8_unchecked(&client.type_name[..client.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&client.type_hash[..client.type_hash_len]);

        let action_info =
            ActionInfo::new(action_name_str, type_str, type_hash_str).with_domain(domain_id);

        // Create send_goal service client
        let send_goal_keyexpr: nros_core::heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, type_str, type_hash_str).with_domain(0);
        let send_goal_client = match session.create_service_client(&send_goal_info) {
            Ok(c) => c,
            Err(_) => return NROS_RET_ERROR,
        };

        // Create cancel_goal service client
        let cancel_goal_keyexpr: nros_core::heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            type_hash_str,
        )
        .with_domain(0);
        let cancel_goal_client = match session.create_service_client(&cancel_goal_info) {
            Ok(c) => c,
            Err(_) => return NROS_RET_ERROR,
        };

        // Create get_result service client
        let get_result_keyexpr: nros_core::heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, type_str, type_hash_str).with_domain(0);
        let get_result_client = match session.create_service_client(&get_result_info) {
            Ok(c) => c,
            Err(_) => return NROS_RET_ERROR,
        };

        // Create feedback subscriber (best-effort QoS)
        let feedback_keyexpr: nros_core::heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, type_str, type_hash_str).with_domain(0);
        let feedback_subscriber =
            match session.create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT) {
                Ok(s) => s,
                Err(_) => return NROS_RET_ERROR,
            };

        // Construct ActionClientCore
        let core = nros_node::ActionClientCore::new(
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
        );

        let internal = alloc::boxed::Box::new(ActionClientInternal { core });
        client._internal = alloc::boxed::Box::into_raw(internal) as *mut _;
    }

    client.state = nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Set feedback callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_set_feedback_callback(
    client: *mut nros_action_client_t,
    callback: nros_feedback_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    if client.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;
    client.feedback_callback = callback;
    client.context = context;

    NROS_RET_OK
}

/// Set result callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_set_result_callback(
    client: *mut nros_action_client_t,
    callback: nros_result_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    if client.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;
    client.result_callback = callback;
    client.context = context;

    NROS_RET_OK
}

/// Send a goal request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_send_goal(
    client: *mut nros_action_client_t,
    goal: *const u8,
    goal_len: usize,
    goal_uuid: *mut nros_goal_uuid_t,
) -> nros_ret_t {
    if client.is_null() || goal.is_null() || goal_uuid.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;

    if client.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);
        let goal_data = core::slice::from_raw_parts(goal, goal_len);

        match internal.core.send_goal_raw(goal_data) {
            Ok(goal_id) => {
                // Copy the generated GoalId to the output UUID
                let uuid = &mut *goal_uuid;
                uuid.uuid = goal_id.uuid;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Request cancellation of a goal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_cancel_goal(
    client: *mut nros_action_client_t,
    goal_uuid: *const nros_goal_uuid_t,
) -> nros_ret_t {
    if client.is_null() || goal_uuid.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;

    if client.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);
        let uuid = &*goal_uuid;
        let goal_id = nros_core::GoalId { uuid: uuid.uuid };

        match internal.core.send_cancel_request(&goal_id) {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Request result of a goal (blocking).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_get_result(
    client: *mut nros_action_client_t,
    goal_uuid: *const nros_goal_uuid_t,
    status: *mut nros_goal_status_t,
    result: *mut u8,
    result_capacity: usize,
    result_len: *mut usize,
) -> nros_ret_t {
    if client.is_null()
        || goal_uuid.is_null()
        || status.is_null()
        || result.is_null()
        || result_len.is_null()
    {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;

    if client.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);
        let uuid = &*goal_uuid;
        let goal_id = nros_core::GoalId { uuid: uuid.uuid };

        // Send the get_result request
        if internal.core.send_get_result_request(&goal_id).is_err() {
            return NROS_RET_ERROR;
        }

        // Poll for the reply with timeout (same approach as nros_client_call)
        let mut attempts = 0u32;
        loop {
            match internal.core.try_recv_get_result_reply() {
                Ok(Some(len)) => {
                    // GetResult response CDR: header (4) + status (int8=1 + 3 pad) + result
                    if len < 4 {
                        return NROS_RET_ERROR;
                    }

                    // Status is the first byte after CDR header
                    let buf = internal.core.result_buffer_ref();
                    let raw_status = buf[4];
                    *status = match raw_status {
                        1 => nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED,
                        2 => nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING,
                        3 => nros_goal_status_t::NROS_GOAL_STATUS_CANCELING,
                        4 => nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
                        5 => nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
                        6 => nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
                        _ => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
                    };

                    // Result data starts after CDR header (4) + status (1) + padding (3)
                    let result_offset = 8usize;
                    let result_data_len = len.saturating_sub(result_offset);

                    if result_data_len > result_capacity {
                        return NROS_RET_ERROR;
                    }

                    let out = core::slice::from_raw_parts_mut(result, result_capacity);
                    out[..result_data_len]
                        .copy_from_slice(&buf[result_offset..result_offset + result_data_len]);
                    *result_len = result_data_len;

                    return NROS_RET_OK;
                }
                Ok(None) => {
                    attempts += 1;
                    if attempts > 3000 {
                        return NROS_RET_TIMEOUT;
                    }
                    #[cfg(feature = "std")]
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    #[cfg(not(feature = "std"))]
                    core::hint::spin_loop();
                }
                Err(_) => return NROS_RET_ERROR,
            }
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Try to receive feedback for an active goal (non-blocking).
///
/// If feedback is available, invokes the feedback callback (if set).
/// Returns `NROS_RET_OK` if feedback was received and dispatched,
/// `NROS_RET_TIMEOUT` if no feedback is available.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_try_recv_feedback(
    client: *mut nros_action_client_t,
) -> nros_ret_t {
    if client.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;

    if client.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    #[cfg(feature = "alloc")]
    {
        if client._internal.is_null() {
            return NROS_RET_NOT_INIT;
        }

        let internal = &mut *(client._internal as *mut ActionClientInternal);

        match internal.core.try_recv_feedback_raw() {
            Ok(Some((goal_id, len))) => {
                // Feedback CDR contains: CDR header (4) + GoalId (16) + feedback data
                // The GoalId has already been parsed by try_recv_feedback_raw.
                // The full raw data (including CDR header + GoalId) is in feedback_buffer.
                // We need to extract just the feedback payload for the C callback.
                // After CDR header (4) + GoalId (16 bytes via CDR) = ~24 bytes offset
                // However, the exact offset depends on CDR encoding. The feedback_buffer
                // contains the full received message. For the C callback, pass the raw
                // feedback bytes starting after the GoalId framing.

                if let Some(cb) = client.feedback_callback {
                    let uuid = nros_goal_uuid_t { uuid: goal_id.uuid };

                    // Feedback CDR layout: header (4) + GoalId UUID (16) = 20 bytes
                    let fb_offset = 20usize;
                    let fb_len = len.saturating_sub(fb_offset);
                    let fb_ptr = if fb_len > 0 {
                        internal.core.feedback_buffer_ref()[fb_offset..].as_ptr()
                    } else {
                        ptr::null()
                    };

                    cb(&uuid, fb_ptr, fb_len, client.context);
                }

                NROS_RET_OK
            }
            Ok(None) => NROS_RET_TIMEOUT,
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    NROS_RET_ERROR
}

/// Finalize an action client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_fini(client: *mut nros_action_client_t) -> nros_ret_t {
    if client.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let client = &mut *client;

    if client.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Drop the internal ActionClientCore
    #[cfg(feature = "alloc")]
    {
        if !client._internal.is_null() {
            let _internal =
                alloc::boxed::Box::from_raw(client._internal as *mut ActionClientInternal);
            // ActionClientInternal (and its ActionClientCore) is dropped here
        }
    }

    client._internal = ptr::null_mut();
    client.feedback_callback = None;
    client.result_callback = None;
    client.context = ptr::null_mut();
    client.node = ptr::null();
    client.state = nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Generate a new random goal UUID.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_goal_uuid_generate(uuid: *mut nros_goal_uuid_t) -> nros_ret_t {
    if uuid.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let uuid = &mut *uuid;

    #[cfg(feature = "std")]
    {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Simple UUID generation using system time and a counter
        // Not cryptographically secure, but sufficient for goal IDs
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let nanos = now.as_nanos() as u64;
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Fill UUID with time-based values
        uuid.uuid[0..8].copy_from_slice(&nanos.to_le_bytes());
        uuid.uuid[8..16].copy_from_slice(&count.to_le_bytes());

        // Set version (4) and variant bits for UUID v4-like format
        uuid.uuid[6] = (uuid.uuid[6] & 0x0f) | 0x40;
        uuid.uuid[8] = (uuid.uuid[8] & 0x3f) | 0x80;

        NROS_RET_OK
    }

    #[cfg(not(feature = "std"))]
    {
        // For no_std, use a simple counter-based approach
        static mut COUNTER: u64 = 0;
        COUNTER = COUNTER.wrapping_add(1);

        uuid.uuid = [0u8; 16];
        let counter_bytes = COUNTER.to_le_bytes();
        uuid.uuid[0..8].copy_from_slice(&counter_bytes);

        NROS_RET_OK
    }
}

/// Compare two goal UUIDs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_goal_uuid_equal(
    a: *const nros_goal_uuid_t,
    b: *const nros_goal_uuid_t,
) -> bool {
    if a.is_null() || b.is_null() {
        return false;
    }

    let a = &*a;
    let b = &*b;

    a.uuid == b.uuid
}

/// Get status name as string.
#[unsafe(no_mangle)]
pub extern "C" fn nros_goal_status_to_string(status: nros_goal_status_t) -> *const c_char {
    match status {
        nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN => c"UNKNOWN".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED => c"ACCEPTED".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING => c"EXECUTING".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_CANCELING => c"CANCELING".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED => c"SUCCEEDED".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_CANCELED => c"CANCELED".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_ABORTED => c"ABORTED".as_ptr(),
    }
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

    // -- Action Server Harnesses --

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
        assert!(srv._internal.is_null());
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

    // -- Action Client Harnesses --

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_init_null_ptrs() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::nros_node_get_zero_initialized();

        // NULL client
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    ptr::null_mut(),
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL node
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    &mut cli,
                    ptr::null(),
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL action_name
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_client_init(&mut cli, &node, ptr::null(), &type_info) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL type_info
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    &mut cli,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    ptr::null(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_init_uninit_node() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::nros_node_get_zero_initialized();

        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_client_init(
                    &mut cli,
                    &node,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    &type_info,
                )
            },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_zero_initialized_state() {
        let cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            cli.state,
            nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_UNINITIALIZED,
        );
        assert!(cli.node.is_null());
        assert!(cli._internal.is_null());
        assert!(cli.feedback_callback.is_none());
        assert!(cli.result_callback.is_none());
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_fini_null_safety() {
        // NULL → INVALID_ARGUMENT
        assert_eq!(
            unsafe { nros_action_client_fini(ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // UNINITIALIZED → NOT_INIT
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_client_fini(&mut cli) },
            NROS_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_send_goal_null_ptrs() {
        let goal_data = [0u8; 8];
        let mut uuid = nros_goal_uuid_t::default();

        // NULL client
        assert_eq!(
            unsafe { nros_action_send_goal(ptr::null_mut(), goal_data.as_ptr(), 8, &mut uuid) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL goal
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe { nros_action_send_goal(&mut cli, ptr::null(), 0, &mut uuid) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL uuid
        assert_eq!(
            unsafe { nros_action_send_goal(&mut cli, goal_data.as_ptr(), 8, ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_get_result_null_ptrs() {
        let uuid = nros_goal_uuid_t::default();
        let mut status = nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN;
        let mut result_buf = [0u8; 8];
        let mut result_len: usize = 0;

        // NULL client
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    ptr::null_mut(),
                    &uuid,
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL uuid
        let mut cli = nros_action_client_get_zero_initialized();
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    ptr::null(),
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL status
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    &uuid,
                    ptr::null_mut(),
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL result
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    &uuid,
                    &mut status,
                    ptr::null_mut(),
                    8,
                    &mut result_len,
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL result_len
        assert_eq!(
            unsafe {
                nros_action_get_result(
                    &mut cli,
                    &uuid,
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    ptr::null_mut(),
                )
            },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    // -- Goal Handle Harnesses --

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

    // -- UUID / Utility Harnesses --

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_uuid_generate_null() {
        assert_eq!(
            unsafe { nros_goal_uuid_generate(ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_uuid_equal_null() {
        let uuid = nros_goal_uuid_t::default();

        assert!(!unsafe { nros_goal_uuid_equal(ptr::null(), &uuid) });
        assert!(!unsafe { nros_goal_uuid_equal(&uuid, ptr::null()) });
        assert!(!unsafe { nros_goal_uuid_equal(ptr::null(), ptr::null()) });
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_status_to_string_all_variants() {
        let statuses = [
            nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
            nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED,
            nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING,
            nros_goal_status_t::NROS_GOAL_STATUS_CANCELING,
            nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
            nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
            nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
        ];

        for status in statuses {
            let s = nros_goal_status_to_string(status);
            assert!(!s.is_null());
        }
    }
}
