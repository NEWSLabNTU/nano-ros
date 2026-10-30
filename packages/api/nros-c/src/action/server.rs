//! Action server implementation.

use core::{ffi::c_void, ptr};

use nros::cdr::strip_cdr_header;

use super::common::*;
use crate::{
    constants::{MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN},
    error::*,
    node::{rcl_node_t, rclc_node_state_t},
};

use nros_node::{
    action_frame,
    executor::ffi::{ActionServerCoreFfi as _, ActionServerRawHandleFfi as _},
};

// ============================================================================
// Action Server
// ============================================================================

/// Action server state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_action_server_state_t {
    /// Not initialized
    RCLC_ACTION_SERVER_STATE_UNINITIALIZED = 0,
    /// Initialized. For a `rcl_action_server_t` the transport is created
    /// by `rclc_executor_attach_action_server`; for a
    /// `rclc_polling_action_server_t` its `ActionServerCore` is live in the
    /// struct.
    RCLC_ACTION_SERVER_STATE_INITIALIZED = 1,
    /// Shutdown
    RCLC_ACTION_SERVER_STATE_SHUTDOWN = 2,
}

/// An action server the executor dispatches.
///
/// phase-487 W4 (RFC-0096 D9, the C half): the executor arena owns the five
/// channels, the goal table and every buffer; this struct holds the arena
/// handle (`_internal`), the registration description and the one callback
/// record, which the arena's trampolines read with this struct as their
/// context. That record stays here, as the C++ action server's does
/// (phase-456 W3b-ii refused moving it into the arena: the trampolines
/// translate the ABI and reframe the goal payload). A server the caller
/// drives itself is a [`rclc_polling_action_server_t`].
#[repr(C)]
pub struct rcl_action_server_t {
    /// Current state
    pub state: rclc_action_server_state_t,
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
    /// Issue 1913 — the per-endpoint hashes, copied from the type support at
    /// init (static-storage pointers; NULL falls back to `type_hash`).
    pub endpoint_hashes: crate::action::common::rclc_action_endpoint_hashes_t,
    /// Goal callback
    pub goal_callback: rclc_goal_callback_t,
    /// Cancel callback
    pub cancel_callback: rclc_cancel_callback_t,
    /// Accepted callback
    pub accepted_callback: rclc_accepted_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: crate::node::rclc_node_ref_t,
    /// Phase 193.4b — action-server QoS, applied to the three underlying
    /// service servers (send_goal / cancel_goal / get_result). The feedback +
    /// status publishers keep their own profiles. Defaults to the services
    /// profile (RELIABLE+VOLATILE+KEEP_LAST(10)); set via
    /// `rclc_action_server_init_with_qos`.
    pub qos: crate::qos::rclc_qos_t,
    /// Phase 189.M3.3.b — scheduling-context slot to bind the action server's
    /// executor handle to (the goal-service slot; governs the action's callback
    /// dispatch). `0` = inherit the executor / Node default; set via
    /// `rclc_action_server_init_with_options`. When non-zero,
    /// `rclc_executor_attach_action_server` binds the handle after
    /// registration. No effect on the L1 polling path.
    pub options: rcl_action_server_options_t,
    /// Internal state — set by `rclc_executor_attach_action_server`.
    /// Typed C-ABI handle field (was an opaque blob in earlier versions).
    pub _internal: ActionServerInternal,
    /// phase-515 C4b — the descriptor this server was created with, kept so
    /// rclc's typed API places a goal, result or feedback in its envelope
    /// through the descriptor's codec and layout. A generated `static const`.
    pub type_support: *const rclc_action_type_t,
    /// phase-515 C4b — rclc's typed registration
    /// (`rclc_executor_add_action_server`): its callbacks and goal handles.
    /// Unused by a server registered through `rclc_action_server_init_with_callbacks`.
    pub _rclc: rclc_action_server_rclc_state_t,
}

/// rclc's `rclc_action_server_t` (phase-515 C4b). One struct serves rcl's
/// spelling and rclc's: `rclc_action_server_init_default` and
/// `rclc_executor_add_action_server` take this.
pub type rclc_action_server_t = rcl_action_server_t;

/// rclc's `rclc_action_server_handle_goal_callback_t`: decide a goal. Return
/// `RCL_RET_ACTION_GOAL_ACCEPTED` to accept it; anything else rejects it.
pub type rclc_action_server_handle_goal_callback_t = Option<
    unsafe extern "C" fn(
        goal_handle: *mut rclc_action_goal_handle_t,
        context: *mut c_void,
    ) -> rcl_ret_t,
>;

/// rclc's `rclc_action_server_handle_cancel_callback_t`: decide a cancel
/// request. `true` accepts it (the goal then ends with
/// `rclc_action_send_result(..., GOAL_STATE_CANCELED, ...)`).
pub type rclc_action_server_handle_cancel_callback_t = Option<
    unsafe extern "C" fn(goal_handle: *mut rclc_action_goal_handle_t, context: *mut c_void) -> bool,
>;

/// rclc's `rclc_action_goal_handle_t` (phase-515 C4b): one goal of an rclc
/// action server, handed to the goal and cancel callbacks and back to
/// `rclc_action_send_result` / `rclc_action_publish_feedback`.
///
/// ADOPT-BOUNDED: rclc's members a server reads are here with rclc's names and
/// types (`status`, `goal_id`, `ros_goal_request`, `goal_accepted`,
/// `goal_cancelled`, `action_server`). `ros_goal_request` is a `void *` (rclc
/// types it `struct Generic_SendGoal_Request *`; both are cast to the user's
/// `<Action>_SendGoal_Request *`). rclc's request headers and its
/// action-client member are not: the runtime answers each request itself, and
/// rclc's typed action client is not ported. The handles live in the server
/// (at most `NROS_MAX_CONCURRENT_GOALS`), not in executor-allocated memory.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct rclc_action_goal_handle_t {
    /// rclc's free/used list link. Always NULL here: the pool is an array.
    pub next: *mut rclc_action_goal_handle_t,
    /// The server this goal arrived on.
    pub action_server: *mut rcl_action_server_t,
    /// The goal's status. `GOAL_STATE_EXECUTING` once accepted,
    /// `GOAL_STATE_CANCELING` while a cancel request is being decided or has
    /// been accepted, terminal once `rclc_action_send_result` sent it.
    pub status: rcl_action_goal_state_t,
    /// The goal's id.
    pub goal_id: unique_identifier_msgs__msg__UUID,
    /// The caller's `<Action>_SendGoal_Request` this goal was deserialised
    /// into, an element of the array `rclc_executor_add_action_server` took.
    pub ros_goal_request: *mut c_void,
    /// rclc's flag; not set by a server.
    pub available_goal_response: bool,
    /// `true` once the goal callback accepted the goal.
    pub goal_accepted: bool,
    /// rclc's flag; not set by a server.
    pub available_feedback: bool,
    /// rclc's flag; not set by a server.
    pub available_result_response: bool,
    /// rclc's flag; not set by a server.
    pub available_cancel_response: bool,
    /// `true` once the cancel callback accepted a cancel request for this goal.
    pub goal_cancelled: bool,
    /// Ours: the handle holds a live goal.
    pub _in_use: bool,
}

impl rclc_action_goal_handle_t {
    const FREE: Self = Self {
        next: ptr::null_mut(),
        action_server: ptr::null_mut(),
        status: GOAL_STATE_UNKNOWN,
        goal_id: rclc_goal_uuid_t { uuid: [0; 16] },
        ros_goal_request: ptr::null_mut(),
        available_goal_response: false,
        goal_accepted: false,
        available_feedback: false,
        available_result_response: false,
        available_cancel_response: false,
        goal_cancelled: false,
        _in_use: false,
    };
}

/// The rclc registration's state, inside the server (phase-515 C4b).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct rclc_action_server_rclc_state_t {
    /// rclc's goal callback.
    pub goal_callback: rclc_action_server_handle_goal_callback_t,
    /// rclc's cancel callback.
    pub cancel_callback: rclc_action_server_handle_cancel_callback_t,
    /// The context both receive.
    pub context: *mut c_void,
    /// The caller's `<Action>_SendGoal_Request` array.
    pub goal_requests: *mut u8,
    /// Its element size.
    pub goal_request_size: usize,
    /// How many goal handles the registration asked for.
    pub handles_number: usize,
    /// The goal handles.
    pub goal_handles: [rclc_action_goal_handle_t; crate::constants::NROS_MAX_CONCURRENT_GOALS],
}

impl rclc_action_server_rclc_state_t {
    const NONE: Self = Self {
        goal_callback: None,
        cancel_callback: None,
        context: ptr::null_mut(),
        goal_requests: ptr::null_mut(),
        goal_request_size: 0,
        handles_number: 0,
        goal_handles: [rclc_action_goal_handle_t::FREE;
            crate::constants::NROS_MAX_CONCURRENT_GOALS],
    };
}

/// An action server the CALLER drives, with no executor (phase-487 W4).
///
/// The C counterpart of C++'s `nros::PollingActionServer<A>`. `_opaque` holds
/// an `ActionServerCore` (five channels, goal table, buffers) from
/// `rclc_polling_action_server_init` until
/// `rclc_polling_action_server_fini`. Zero-initialised is UNINITIALIZED.
/// Until phase-487 it was the ~~`NROS_ACTION_SERVER_STATE_POLLING`~~ state of
/// `rcl_action_server_t`.
#[repr(C)]
pub struct rclc_polling_action_server_t {
    /// Current state: UNINITIALIZED, INITIALIZED (core live) or SHUTDOWN.
    pub state: rclc_action_server_state_t,
    /// The node the channels were created on.
    pub node: crate::node::rclc_node_ref_t,
    /// Inline storage for the `ActionServerCore`. Internal: treat as opaque.
    pub _opaque: [u64; crate::opaque_sizes::ACTION_SERVER_OPAQUE_U64S],
}

impl Default for rclc_polling_action_server_t {
    fn default() -> Self {
        Self {
            state: rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_UNINITIALIZED,
            node: crate::node::rclc_node_ref_t::none(),
            _opaque: [0u64; crate::opaque_sizes::ACTION_SERVER_OPAQUE_U64S],
        }
    }
}

/// Get a zero-initialized polling action server.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_get_zero_initialized_polling_action_server() -> rclc_polling_action_server_t
{
    rclc_polling_action_server_t::default()
}

impl Default for rcl_action_server_t {
    fn default() -> Self {
        Self {
            state: rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_UNINITIALIZED,
            action_name: [0u8; MAX_ACTION_NAME_LEN],
            action_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            endpoint_hashes: crate::action::common::rclc_action_endpoint_hashes_t::NULL,
            goal_callback: None,
            cancel_callback: None,
            accepted_callback: None,
            context: ptr::null_mut(),
            node: crate::node::rclc_node_ref_t::none(),
            qos: crate::qos::rclc_qos_t::default(),
            options: rcl_action_server_options_t::zeroed(),
            _internal: ActionServerInternal::invalid_default(),
            type_support: ptr::null(),
            _rclc: rclc_action_server_rclc_state_t::NONE,
        }
    }
}

impl rcl_action_server_t {
    /// Phase 193.4b — the server's QoS as `nros_node` settings.
    pub(crate) fn get_qos_settings(&self) -> nros_rmw::QoSProfile {
        self.qos.to_qos_settings()
    }
}

// ============================================================================
// Internal implementation (delegates to nros-node executor)
// ============================================================================

/// The arena handle, filled in by executor registration.
///
/// `handle` was `Option<ActionServerRawHandle>` previously; now it is
/// always present, with the sentinel `INVALID_ENTRY_INDEX` indicating
/// "not registered yet". Use `is_handle_set()` to check.
///
/// phase-487 W4 — this used to carry a SECOND copy of the server's callback
/// record (~~`c_goal_callback`~~, ~~`c_cancel_callback`~~, ~~`c_accepted_callback`~~,
/// `c_context`) plus a `server_ptr` back to the struct that contains it, so
/// the trampolines could take `&_internal` as their context. The trampolines
/// take the server itself now and read the one record it already has.
#[repr(C)]
pub struct ActionServerInternal {
    /// Handle returned by executor registration. `entry_index ==
    /// INVALID_ENTRY_INDEX` until registration completes.
    pub handle: nros_node::ActionServerRawHandle,
    /// Pointer to the internal executor (`CExecutor`).
    pub executor_ptr: *mut c_void,
}

impl ActionServerInternal {
    /// `true` once `handle` has been populated by executor registration.
    #[inline]
    pub fn is_handle_set(&self) -> bool {
        !self.handle.is_invalid()
    }

    /// Construct a zero-initialised internal — sentinel handle, null
    /// executor. Used for the default state of `rcl_action_server_t._internal`.
    pub fn invalid_default() -> Self {
        Self {
            handle: nros_node::ActionServerRawHandle::invalid(),
            executor_ptr: ptr::null_mut(),
        }
    }
}

impl Default for ActionServerInternal {
    fn default() -> Self {
        Self::invalid_default()
    }
}

/// Build a stack-local `rclc_goal_handle_t` from an arena-supplied UUID.
///
/// The handle is a pure ID card (`{uuid}`), so the trampolines can hand
/// user callbacks a `*const rclc_goal_handle_t` that's valid for the
/// duration of the callback. Users copy the handle by value if they need
/// the UUID beyond the callback's lifetime.
unsafe fn handle_from_goal_id(goal_id: *const nros_node::GoalUuid) -> rclc_goal_handle_t {
    rclc_goal_handle_t {
        uuid: rclc_goal_uuid_t {
            uuid: (*goal_id).uuid,
        },
    }
}

/// Goal callback trampoline for nros-node.
///
/// Builds a stack-local `rclc_goal_handle_t` from the incoming goal_id
/// and forwards to the user's `goal_callback` with `(server, goal,
/// request, len, ctx)`. `context` is the `rcl_action_server_t` itself. The handle lives on this stack frame; users that
/// need to reference the goal later copy it by value.
pub(crate) unsafe extern "C" fn goal_callback_trampoline(
    goal_id: *const nros_node::GoalUuid,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut c_void,
) -> nros_node::GoalResponse {
    let server = context as *mut rcl_action_server_t;
    let goal_handle = handle_from_goal_id(goal_id);
    // `rclc_executor_attach_action_server` refuses a server with no goal
    // callback, so this is only reached with one installed.
    let Some(goal_cb) = (*server).goal_callback else {
        return nros_node::GoalResponse::Reject;
    };

    // goal_data contains the full CDR request: [CDR_HDR][UUID][goal_fields].
    // The C callback expects CDR-encoded goal data: [CDR_HDR][goal_fields],
    // built on the stack (must outlive the callback). Issue 2014 — offset and
    // splice from `action_frame` (the request's own header in front).
    let goal_slice = core::slice::from_raw_parts(goal_data, goal_len);
    const GOAL_CB_BUF: usize = 512;
    let mut cb_buf = [0u8; GOAL_CB_BUF];
    let (cb_ptr, cb_len) = match action_frame::splice_goal_request(goal_slice, &mut cb_buf) {
        Some(n) => (cb_buf.as_ptr(), n),
        None => (goal_data, goal_len),
    };

    let c_response = goal_cb(server, &goal_handle, cb_ptr, cb_len, (*server).context);

    match c_response {
        rclc_goal_response_t::RCLC_GOAL_REJECT => nros_node::GoalResponse::Reject,
        rclc_goal_response_t::RCLC_GOAL_ACCEPT_AND_EXECUTE => {
            nros_node::GoalResponse::AcceptAndExecute
        }
        rclc_goal_response_t::RCLC_GOAL_ACCEPT_AND_DEFER => nros_node::GoalResponse::AcceptAndDefer,
    }
}

/// Post-accept trampoline for nros-node.
///
/// Invoked by the arena *after* `ActionServerCore::accept_goal` has sent
/// the accept reply to the client. Forwards to the user's
/// `accepted_callback` with a stack-local handle.
pub(crate) unsafe extern "C" fn accepted_callback_trampoline(
    goal_id: *const nros_node::GoalUuid,
    context: *mut c_void,
) {
    let server = context as *mut rcl_action_server_t;
    if let Some(cb) = (*server).accepted_callback {
        let goal_handle = handle_from_goal_id(goal_id);
        cb(server, &goal_handle, (*server).context);
    }
}

/// Cancel callback trampoline for nros-node.
///
/// Forwards to the user's `cancel_callback` with a stack-local handle.
/// Status transitions (CANCELING, etc.) are performed by the arena in
/// `ActionServerCore`; this trampoline only translates the ABI.
pub(crate) unsafe extern "C" fn cancel_callback_trampoline(
    goal_id: *const nros_node::GoalUuid,
    _status: nros_node::GoalStatusCode,
    context: *mut c_void,
) -> nros_node::CancelResponse {
    let server = context as *mut rcl_action_server_t;

    let Some(cb) = (*server).cancel_callback else {
        // No cancel callback — accept by default.
        return nros_node::CancelResponse::Accept;
    };

    let goal_handle = handle_from_goal_id(goal_id);
    let c_response = cb(server, &goal_handle, (*server).context);
    // issue 0796 — one-to-one now: C's REJECT=0/ACCEPT=1 and Rust's
    // `CancelResponse::{Reject,Accept}` are the same per-goal decision with the
    // same discriminants. This used to translate into the RPC-level return
    // code (`Ok`/`Rejected`), which is why it needed a comment explaining that
    // "Ok" meant "accepted".
    match c_response {
        rclc_cancel_response_t::RCLC_CANCEL_ACCEPT => nros_node::CancelResponse::Accept,
        rclc_cancel_response_t::RCLC_CANCEL_REJECT => nros_node::CancelResponse::Reject,
    }
}

/// Get the `ActionServerInternal` from a server's inline `_internal` storage.
///
/// # Safety
/// The server must have been registered with the executor (state = INITIALIZED
/// and `_internal` written via `ptr::write`).
unsafe fn get_internal(server: *const rcl_action_server_t) -> &'static ActionServerInternal {
    &(*server)._internal
}

// ============================================================================
// Action Server Functions
// ============================================================================

/// Get a zero-initialized action server.
///
/// `improper_ctypes_definitions` is silenced because the inline opaque
/// handle stores a function-pointer field whose parameter signature
/// references a runtime-internal type. The pointer itself is C-ABI
/// safe; the C API never lets callers invoke through it directly.
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn rcl_action_get_zero_initialized_server() -> rcl_action_server_t {
    rcl_action_server_t::default()
}

/// Initialize an action server.
///
/// Stores metadata (name, type, callbacks). RMW entity creation is deferred
/// to `rclc_executor_attach_action_server()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_server_init_with_callbacks(
    server: *mut rcl_action_server_t,
    node: *const rcl_node_t,
    action_name: *const core::ffi::c_char,
    type_info: *const rclc_action_type_t,
    goal_callback: rclc_goal_callback_t,
    cancel_callback: rclc_cancel_callback_t,
    accepted_callback: rclc_accepted_callback_t,
    context: *mut c_void,
) -> rcl_ret_t {
    if goal_callback.is_none() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    unsafe {
        action_server_init_impl(
            server,
            node,
            action_name,
            type_info,
            goal_callback,
            cancel_callback,
            accepted_callback,
            context,
        )
    }
}

/// The one init body: [`rclc_action_server_init_with_callbacks`] (callbacks
/// bound here) and phase-515 C4b's [`rclc_action_server_init_default`] (none
/// yet; `rclc_executor_add_action_server` binds them).
#[allow(clippy::too_many_arguments)]
unsafe fn action_server_init_impl(
    server: *mut rcl_action_server_t,
    node: *const rcl_node_t,
    action_name: *const core::ffi::c_char,
    type_info: *const rclc_action_type_t,
    goal_callback: rclc_goal_callback_t,
    cancel_callback: rclc_cancel_callback_t,
    accepted_callback: rclc_accepted_callback_t,
    context: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(server, node, action_name, type_info);

    let type_info_ptr = type_info;
    let server = &mut *server;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        server,
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_UNINITIALIZED,
        RCLC_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED);

    // Copy action name (required — empty rejected)
    server.action_name_len = crate::util::copy_cstr_into(action_name, &mut server.action_name);
    if server.action_name_len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    server.type_name_len = crate::util::copy_cstr_into(type_info.type_name, &mut server.type_name);
    server.type_hash_len = crate::util::copy_cstr_into(type_info.type_hash, &mut server.type_hash);
    server.endpoint_hashes = type_info.endpoint_hashes;
    server.type_support = type_info_ptr;
    server._rclc = rclc_action_server_rclc_state_t::NONE;

    // Store callbacks and context
    server.goal_callback = goal_callback;
    server.cancel_callback = cancel_callback;
    server.accepted_callback = accepted_callback;
    server.context = context;
    server.node = unsafe { crate::node::node_ref_of(node) };

    // Phase 193.4b — default to the services profile;
    // rclc_action_server_init_with_qos overrides. Read at registration time by
    // rclc_executor_attach_action_server.
    server.qos = crate::qos::rclc_qos_t::default();

    // RMW entity creation is deferred to rclc_executor_attach_action_server()
    server._internal = ActionServerInternal::invalid_default();
    server.state = rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED;

    RCL_RET_OK
}

/// Phase 193.4b — initialize an action server with an explicit QoS profile
/// (rclc's `_with_options`). The profile applies to the three underlying
/// service servers (send_goal / cancel_goal / get_result); the feedback +
/// status publishers keep their own profiles. `qos` NULL ⇒ the services
/// default. Same as [`rclc_action_server_init_with_callbacks`] otherwise.
///
/// # Safety
/// All non-NULL pointers must be valid + the node initialized.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rclc_action_server_init_with_qos(
    server: *mut rcl_action_server_t,
    node: *const rcl_node_t,
    action_name: *const core::ffi::c_char,
    type_info: *const rclc_action_type_t,
    goal_callback: rclc_goal_callback_t,
    cancel_callback: rclc_cancel_callback_t,
    accepted_callback: rclc_accepted_callback_t,
    context: *mut c_void,
    qos: *const crate::qos::rclc_qos_t,
) -> rcl_ret_t {
    let ret = rclc_action_server_init_with_callbacks(
        server,
        node,
        action_name,
        type_info,
        goal_callback,
        cancel_callback,
        accepted_callback,
        context,
    );
    if ret == RCL_RET_OK && !qos.is_null() {
        (*server).qos = *qos;
    }
    ret
}

/// Phase 189.M3.3.b — rclc-style named action-server options. QoS is passed
/// separately; this carries the non-QoS axes. Zero-init = default behaviour.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct rcl_action_server_options_t {
    /// Scheduling-context slot to bind the action server's executor handle to.
    /// `0` = inherit the executor / Node default. A non-zero value must be an id
    /// from `rclc_executor_create_sched_context`; the bind is applied by
    /// `rclc_executor_attach_action_server` once the handle exists. No effect
    /// on the L1 polling path.
    pub sched_context: crate::executor::rclc_sched_context_id_t,
    /// Reserved for future use; must be zero. Pads for ABI stability.
    pub _reserved: [u8; 3],
}

impl rcl_action_server_options_t {
    /// Every field zero, as `Default` gives -- `const`, for the handles'
    /// zero-initialisers (phase-515 C6).
    pub(crate) const fn zeroed() -> Self {
        Self {
            sched_context: 0,
            _reserved: [0; 3],
        }
    }
}

/// rcl's `rcl_action_server_get_options(server)`: the options the action server was
/// initialised with -- all zero when it was initialised without any -- or
/// NULL for a NULL or unusable handle (phase-515 C6).
///
/// ADOPT-BOUNDED: nano-ros's options carry the scheduling context, not rcl's `qos`,
/// `allocator` or rmw-specific options, so a ported `->qos` does not compile;
/// the handle's QoS is the one it was created with.
///
/// # Safety
/// `server` must be NULL or point to a valid `rcl_action_server_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_action_server_get_options(
    server: *const rcl_action_server_t,
) -> *const rcl_action_server_options_t {
    if server.is_null() {
        return core::ptr::null();
    }
    let server = &*server;
    if !matches!(
        server.state,
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED
    ) {
        return core::ptr::null();
    }
    &server.options
}

/// Get a zero-initialised [`rcl_action_server_options_t`] (`sched_context = 0`).
#[unsafe(no_mangle)]
pub extern "C" fn rcl_action_server_get_default_options() -> rcl_action_server_options_t {
    rcl_action_server_options_t::default()
}

/// Phase 189.M3.3.b — initialize an action server with custom QoS + named
/// options. Like [`rclc_action_server_init_with_qos`] except a non-zero
/// `options->sched_context` is stashed so `rclc_executor_attach_action_server`
/// binds the resulting executor handle to that scheduling context once known.
///
/// # Safety
/// All non-NULL pointers must be valid + the node initialized; `qos` / `options`
/// may be NULL.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rclc_action_server_init_with_options(
    server: *mut rcl_action_server_t,
    node: *const rcl_node_t,
    action_name: *const core::ffi::c_char,
    type_info: *const rclc_action_type_t,
    goal_callback: rclc_goal_callback_t,
    cancel_callback: rclc_cancel_callback_t,
    accepted_callback: rclc_accepted_callback_t,
    context: *mut c_void,
    qos: *const crate::qos::rclc_qos_t,
    options: *const rcl_action_server_options_t,
) -> rcl_ret_t {
    let ret = rclc_action_server_init_with_qos(
        server,
        node,
        action_name,
        type_info,
        goal_callback,
        cancel_callback,
        accepted_callback,
        context,
        qos,
    );
    if ret != RCL_RET_OK {
        return ret;
    }
    if !options.is_null() {
        (*server).options = *options;
    }
    RCL_RET_OK
}

/// Publish feedback for an active goal.
///
/// Delegates to `ActionServerRawHandle::publish_feedback_raw`; the arena
/// enforces goal liveness and is the sole source of truth for lifecycle
/// state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_publish_feedback_raw(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
    feedback: *const u8,
    feedback_len: usize,
) -> rcl_ret_t {
    validate_not_null!(server, goal, feedback);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return RCL_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let mut executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalUuid {
        uuid: (*goal).uuid.uuid,
    };
    let data = core::slice::from_raw_parts(feedback, feedback_len);

    // C serialize produces [CDR_HEADER][fields], but publish_feedback_raw
    // expects raw fields only (it adds its own CDR header + GoalUuid framing).
    let fields = strip_cdr_header(data);

    match handle.publish_feedback_raw(&mut executor, &goal_id, fields) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCL_RET_ERROR,
    }
}

/// Mark a goal as succeeded with a result.
///
/// Delegates to `ActionServerRawHandle::complete_goal_raw`; the arena
/// owns the active-goals vector and retires the goal there.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_succeed(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> rcl_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return RCL_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let mut executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalUuid {
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

    // issue 0796 — a result the server cannot retain (larger than the action
    // server's RESULT_BUF) is now reported instead of silently dropped.
    match handle.complete_goal_raw(
        &mut executor,
        &goal_id,
        nros_node::GoalStatusCode::Succeeded,
        result_fields,
    ) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCL_RET_ERROR,
    }
}

/// Mark a goal as aborted with an optional result.
///
/// Delegates to `ActionServerRawHandle::complete_goal_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_abort(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> rcl_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return RCL_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let mut executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalUuid {
        uuid: (*goal).uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER][fields] — strip the header.
    let result_fields = strip_cdr_header(result_data);

    // issue 0796 — a result the server cannot retain (larger than the action
    // server's RESULT_BUF) is now reported instead of silently dropped.
    match handle.complete_goal_raw(
        &mut executor,
        &goal_id,
        nros_node::GoalStatusCode::Aborted,
        result_fields,
    ) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCL_RET_ERROR,
    }
}

/// Mark a goal as canceled with an optional result.
///
/// Delegates to `ActionServerRawHandle::complete_goal_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_canceled(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
    result: *const u8,
    result_len: usize,
) -> rcl_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return RCL_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let mut executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalUuid {
        uuid: (*goal).uuid.uuid,
    };
    let result_data = if !result.is_null() {
        core::slice::from_raw_parts(result, result_len)
    } else {
        &[]
    };

    // C serialize produces [CDR_HEADER][fields] — strip the header.
    let result_fields = strip_cdr_header(result_data);

    // issue 0796 — a result the server cannot retain (larger than the action
    // server's RESULT_BUF) is now reported instead of silently dropped.
    match handle.complete_goal_raw(
        &mut executor,
        &goal_id,
        nros_node::GoalStatusCode::Canceled,
        result_fields,
    ) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCL_RET_ERROR,
    }
}

/// Execute a goal (transition to `Executing`).
///
/// Idempotent: delegates to `ActionServerRawHandle::set_goal_status` which
/// is a no-op if the goal isn't in the active-goals vector. Returns
/// `RCL_RET_OK` on success, `RCL_RET_NOT_INIT` if the server isn't
/// registered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_execute(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
) -> rcl_ret_t {
    validate_not_null!(server, goal);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return RCL_RET_NOT_INIT;
    }
    let handle = internal.handle;

    let mut executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalUuid {
        uuid: (*goal).uuid.uuid,
    };
    handle.set_goal_status(
        &mut executor,
        &goal_id,
        nros_node::GoalStatusCode::Executing,
    );
    RCL_RET_OK
}

/// Get the number of currently active goals.
///
/// Phase-379 W6 decision 3: ONE function in rcl's shape, serving both
/// tiers. Upstream ships no count function; its nearest neighbour is
/// `rcl_action_server_get_goal_handles(server, ***handles, size_t
/// *num_goals) -> rcl_ret_t`, which reports the count through an
/// out-param and keeps the return code for "did the query work". This
/// replaces the pair `rclc_action_server_get_active_goal_count(server)
/// -> size_t` (answered 0 on every error) and
/// ~~`nros_action_server_active_goal_count_raw(server) -> int32_t`~~
/// (negative = error code), each of which collapsed the two channels
/// into one value.
///
/// Reads the executor arena through
/// `ActionServerRawHandle::active_goal_count`. A polled server, whose
/// `ActionServerCore` is inline, answers through
/// [`rclc_polling_action_server_get_active_goal_count`]; until phase-487 W4
/// both tiers shared this entry point and were told apart by state.
///
/// Returns:
///
/// * `RCL_RET_OK` — `*out` holds the count, which may legitimately be 0.
/// * `RCL_RET_INVALID_ARGUMENT` — `server` or `out` is NULL.
/// * `RCL_RET_NOT_INIT` — the server is UNINITIALIZED or SHUTDOWN; or it
///   is INITIALIZED but was never registered with an executor (or has
///   been finalised).
///
/// `*out` is left untouched on every error path, so a caller that ignores
/// the return code reads back its own initialiser rather than a
/// fabricated `0` — which is precisely the ambiguity the old `size_t`
/// spelling could not escape.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_server_get_active_goal_count(
    server: *mut rcl_action_server_t,
    out: *mut usize,
) -> rcl_ret_t {
    validate_not_null!(server, out);

    match (*server).state {
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED => {
            let internal = get_internal(server);
            if !internal.is_handle_set() {
                return RCL_RET_NOT_INIT;
            }
            let handle = internal.handle;
            let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
            *out = handle.active_goal_count(&executor);
            RCL_RET_OK
        }
        _ => RCL_RET_NOT_INIT,
    }
}

/// Read back the action name this server was created on.
///
/// rcl's `rcl_action_server_get_action_name`, the server twin of
/// [`rcl_action_client_get_action_name`](crate::action::client::rcl_action_client_get_action_name)
/// — phase-417 W4.b, ledger row `c:action_server_get_action_name`. The name is
/// the caller's own string, copied NUL-terminated into the handle at init, so
/// this hands back a pointer into the handle and costs nothing.
///
/// # Parameters
/// * `server` - Pointer to an action server
///
/// # Returns
/// * Pointer to the action name (NUL-terminated), or NULL when `server` is
///   NULL or was never initialised. Both live tiers answer — L2 (callback,
///   `INITIALIZED`) and L1 (polling), for the same reason
///   `rcl_service_t::is_usable` covers both.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_action_server_get_action_name(
    server: *const rcl_action_server_t,
) -> *const core::ffi::c_char {
    if server.is_null() {
        return core::ptr::null();
    }
    let server = &*server;
    match server.state {
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED => {
            server.action_name.as_ptr() as *const core::ffi::c_char
        }
        _ => core::ptr::null(),
    }
}

/// Is `goal` a goal this server still knows about?
///
/// rcl's `rcl_action_server_goal_exists` over OUR goal handle, hence the
/// distinct `rclc_` name (phase-492 W1.b) — phase-417 W4.b, ledger row
/// `c:action_server_goal_exists`. TRUE while the goal is ACTIVE and while its
/// completed result is still retained for a later `get_result`, which is
/// rcl's own window (its goal-handle table keeps a terminated goal until
/// `rcl_action_expire_goals` reclaims it).
///
/// **This is not what [`rclc_action_get_goal_status`] answers**, and the row
/// was open on the guess that it was. That lookup reads the ACTIVE set only,
/// so it returns `RCLC_RET_NOT_FOUND` for a goal that has completed and whose
/// result is still sitting in the slab — i.e. for exactly the goals a client
/// is in the middle of fetching. A ported `if (rclc_action_server_goal_exists
/// (...))` guard written against that mapping would have been wrong in the one
/// window it matters.
///
/// # Parameters
/// * `server` - Pointer to an action server
/// * `goal` - The goal handle (its 16-byte UUID is the key)
///
/// # Returns
/// * `true` if the server knows the goal; `false` for a NULL argument, an
///   uninitialised/finalised server, or an unknown goal. A predicate, so
///   there is no separate error channel — the ported idiom is a guard, the
///   same shape as `rcl_service_is_valid`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_server_goal_exists(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
) -> bool {
    if server.is_null() || goal.is_null() {
        return false;
    }
    let goal_id = nros_node::GoalUuid {
        uuid: (*goal).uuid.uuid,
    };
    match (*server).state {
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED => {
            let internal = get_internal(server);
            if !internal.is_handle_set() {
                return false;
            }
            let handle = internal.handle;
            let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
            handle.goal_exists(&executor, &goal_id)
        }
        _ => false,
    }
}

/// Eagerly reclaim every completed result whose `get_result` reply has already
/// been sent, and report how many were reclaimed.
///
/// rcl's `rcl_action_expire_goals` — phase-417 W4.b, ledger row
/// `c:action_expire_goals`. **ADOPT-BOUNDED, and the envelope is the whole
/// point of the row**: upstream's trigger is a CLOCK (a goal expires
/// `result_timeout` after it terminates, 15 min by default) and it fills a
/// caller-supplied `rcl_action_goal_info_t` array with the goals that went.
/// Neither half survives here:
///
/// * **No timeout.** `ActionServerCore` is `no_std` with no time source
///   threaded through it (RFC-0036), so there is nothing to measure a
///   `result_timeout` against. The trigger is the CALLER, and reclamation
///   otherwise happens on demand when a completion needs slab room — which is
///   what already bounds the memory upstream's timer exists to bound.
/// * **No `expired_goals` report.** An entry is reclaimed the moment its
///   result has been delivered, so "which goals expired" is a question about
///   a clock we do not have; the count is what remains true.
///
/// Calling this is OPTIONAL. It exists for a server that would rather return
/// the slab eagerly (before a long idle period, say) than at the next
/// completion.
///
/// # Parameters
/// * `server` - Pointer to an action server
/// * `num_expired` - Receives the number of results reclaimed (may be 0)
///
/// # Returns
/// * `RCL_RET_OK` — `*num_expired` holds the count, which may legitimately be 0.
/// * `RCL_RET_INVALID_ARGUMENT` — `server` or `num_expired` is NULL.
/// * `RCL_RET_NOT_INIT` — the server is UNINITIALIZED or SHUTDOWN, was never
///   registered with an executor.
///
/// `*num_expired` is left untouched on every error path, for the same reason
/// [`rclc_action_server_get_active_goal_count`] leaves its own out-parameter
/// alone.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_expire_goals(
    server: *mut rcl_action_server_t,
    num_expired: *mut usize,
) -> rcl_ret_t {
    validate_not_null!(server, num_expired);

    match (*server).state {
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED => {
            let internal = get_internal(server);
            if !internal.is_handle_set() {
                return RCL_RET_NOT_INIT;
            }
            let handle = internal.handle;
            let mut executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
            *num_expired = handle.expire_goals(&mut executor);
            RCL_RET_OK
        }
        _ => RCL_RET_NOT_INIT,
    }
}

/// Look up a goal's current status in the arena by UUID.
///
/// Returns `RCL_RET_OK` and writes the arena-sourced status on success.
/// Returns `RCLC_RET_NOT_FOUND` if the arena has already retired the goal
/// (completed + result delivered, or cancelled + acknowledged).
///
/// This is the authoritative successor to the removed `goal->status`
/// field: lifecycle transitions are driven by
/// `ActionServerArenaEntry::active_goals` and read back through this
/// function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_get_goal_status(
    server: *const rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
    status: *mut rclc_goal_status_t,
) -> rcl_ret_t {
    validate_not_null!(server, goal, status);

    let internal = get_internal(server);
    if !internal.is_handle_set() {
        return RCL_RET_NOT_INIT;
    }
    let handle = internal.handle;
    let executor = crate::executor::get_executor_from_ptr(internal.executor_ptr);
    let goal_id = nros_node::GoalUuid {
        uuid: (*goal).uuid.uuid,
    };
    match handle.goal_status(&executor, &goal_id) {
        Some(s) => {
            *status = goal_status_from_core(s);
            RCL_RET_OK
        }
        None => RCLC_RET_NOT_FOUND,
    }
}

/// Map a `nros_node::GoalStatusCode` to its `rclc_goal_status_t` equivalent.
fn goal_status_from_core(status: nros_node::GoalStatusCode) -> rclc_goal_status_t {
    use nros_node::GoalStatusCode;
    match status {
        GoalStatusCode::Unknown => rclc_goal_status_t::RCLC_GOAL_STATUS_UNKNOWN,
        GoalStatusCode::Accepted => rclc_goal_status_t::RCLC_GOAL_STATUS_ACCEPTED,
        GoalStatusCode::Executing => rclc_goal_status_t::RCLC_GOAL_STATUS_EXECUTING,
        GoalStatusCode::Canceling => rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELING,
        GoalStatusCode::Succeeded => rclc_goal_status_t::RCLC_GOAL_STATUS_SUCCEEDED,
        GoalStatusCode::Canceled => rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELED,
        GoalStatusCode::Aborted => rclc_goal_status_t::RCLC_GOAL_STATUS_ABORTED,
    }
}

/// Phase 122.3.c.6.b — reverse of `goal_status_from_core`.
fn c_status_to_rust(status: rclc_goal_status_t) -> nros_node::GoalStatusCode {
    use nros_node::GoalStatusCode;
    match status {
        rclc_goal_status_t::RCLC_GOAL_STATUS_UNKNOWN => GoalStatusCode::Unknown,
        rclc_goal_status_t::RCLC_GOAL_STATUS_ACCEPTED => GoalStatusCode::Accepted,
        rclc_goal_status_t::RCLC_GOAL_STATUS_EXECUTING => GoalStatusCode::Executing,
        rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELING => GoalStatusCode::Canceling,
        rclc_goal_status_t::RCLC_GOAL_STATUS_SUCCEEDED => GoalStatusCode::Succeeded,
        rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELED => GoalStatusCode::Canceled,
        rclc_goal_status_t::RCLC_GOAL_STATUS_ABORTED => GoalStatusCode::Aborted,
    }
}

/// Finalize an action server.
///
/// Issue 1609 — `fini` does NOT remove the server from an executor. It is
/// handed only the entity, and an executor-registered server's arena entry
/// (its three service servers, two publishers and the callback `context`,
/// which is `server` itself) is undone by
/// `rclc_executor_remove_action_server(executor, server)`, which also brings
/// the executor's `handle_count` back down. Call that first; a server that is
/// finalised while still registered keeps answering goals into this struct
/// until `rclc_executor_fini`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_action_server_fini(
    server: *mut rcl_action_server_t,
    node: *mut crate::node::rcl_node_t,
) -> rcl_ret_t {
    // phase-492 W1.b — rcl's arity: the node the entity was created on.
    // Checked non-NULL only; teardown needs nothing from it (RFC-0089
    // ADOPT-BOUNDED, ledger `c:action_server_fini`).
    validate_not_null!(server, node);

    let server = &mut *server;

    match server.state {
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED => {
            // L2: action server lives in executor arena (if registered) —
            // reset metadata only. Releasing the entry needs the owning
            // `rclc_executor_t` (issue 1609): that is
            // `rclc_executor_remove_action_server`.
        }
        _ => return RCL_RET_NOT_INIT,
    }

    server._internal = ActionServerInternal::invalid_default();
    server.goal_callback = None;
    server.cancel_callback = None;
    server.accepted_callback = None;
    server.context = ptr::null_mut();
    server._rclc = rclc_action_server_rclc_state_t::NONE;
    server.node = crate::node::rclc_node_ref_t::none();
    server.state = rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_SHUTDOWN;

    RCL_RET_OK
}

// ============================================================================
// phase-515 C4b — rclc's typed action server
// ============================================================================
//
// rclc's model over the raw registration: `rclc_executor_add_action_server`
// installs the two trampolines below as the server's goal and cancel callbacks,
// and they run rclc's goal-handle bookkeeping on the server's pool before
// calling the user's callbacks. A goal is deserialised into the caller's own
// `<Action>_SendGoal_Request`; a result and a feedback sample are serialised
// out of the caller's `<Action>_GetResult_Response` / `<Action>_FeedbackMessage`
// at the offsets the generated descriptor states (phase-515 C4a).

/// The handle of `goal_id` among `server`'s live rclc goals.
fn rclc_goal_handle_for<'a>(
    server: &'a mut rcl_action_server_t,
    goal_id: &rclc_goal_uuid_t,
) -> Option<&'a mut rclc_action_goal_handle_t> {
    let n = server
        ._rclc
        .handles_number
        .min(crate::constants::NROS_MAX_CONCURRENT_GOALS);
    server._rclc.goal_handles[..n]
        .iter_mut()
        .find(|h| h._in_use && h.goal_id.uuid == goal_id.uuid)
}

/// The goal callback an rclc registration installs: take a free goal handle,
/// deserialise the goal into its element of the caller's array, ask the
/// user's callback.
unsafe extern "C" fn rclc_goal_trampoline(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
    goal_request: *const u8,
    goal_len: usize,
    _context: *mut c_void,
) -> rclc_goal_response_t {
    let reject = rclc_goal_response_t::RCLC_GOAL_REJECT;
    if server.is_null() || goal.is_null() {
        return reject;
    }
    let srv = unsafe { &mut *server };
    let (Some(callback), Some(type_info)) = (srv._rclc.goal_callback, unsafe {
        srv.type_support.as_ref()
    }) else {
        return reject;
    };
    let Some(deserialize) = type_info.goal_deserialize else {
        return reject;
    };
    let n = srv
        ._rclc
        .handles_number
        .min(crate::constants::NROS_MAX_CONCURRENT_GOALS);
    // No free handle: rclc leaves the request untaken; there is nowhere to put
    // it here either, so it is rejected now rather than never answered.
    let Some(i) = srv._rclc.goal_handles[..n].iter().position(|h| !h._in_use) else {
        return reject;
    };
    let request = unsafe { srv._rclc.goal_requests.add(i * srv._rclc.goal_request_size) };
    let goal_id = unsafe { (*goal).uuid };
    // `goal_id` is the envelope's first member; the goal sits at the
    // descriptor's offset.
    unsafe { ptr::copy_nonoverlapping(goal_id.uuid.as_ptr(), request, 16) };
    let goal_at = unsafe { request.add(type_info.send_goal_request_goal_offset) } as *mut c_void;
    if unsafe { deserialize(goal_request, goal_len, goal_at) } != 0 {
        return reject;
    }
    let context = srv._rclc.context;
    // Raw from here: the user's callback holds the handle, which lives inside
    // the server, and may call back into this API with it.
    let handle: *mut rclc_action_goal_handle_t = &mut srv._rclc.goal_handles[i];
    unsafe {
        *handle = rclc_action_goal_handle_t {
            action_server: server,
            status: GOAL_STATE_UNKNOWN,
            goal_id,
            ros_goal_request: request as *mut c_void,
            _in_use: true,
            ..rclc_action_goal_handle_t::FREE
        };
        if callback(handle, context) == RCL_RET_ACTION_GOAL_ACCEPTED {
            (*handle).goal_accepted = true;
            (*handle).status = GOAL_STATE_EXECUTING;
            rclc_goal_response_t::RCLC_GOAL_ACCEPT_AND_EXECUTE
        } else {
            *handle = rclc_action_goal_handle_t::FREE;
            reject
        }
    }
}

/// The cancel callback an rclc registration installs: the goal goes
/// `CANCELING` while the user's callback decides; accepted, it stays there
/// with `goal_cancelled` set; refused, it is `EXECUTING` again.
unsafe extern "C" fn rclc_cancel_trampoline(
    server: *mut rcl_action_server_t,
    goal: *const rclc_goal_handle_t,
    _context: *mut c_void,
) -> rclc_cancel_response_t {
    let reject = rclc_cancel_response_t::RCLC_CANCEL_REJECT;
    if server.is_null() || goal.is_null() {
        return reject;
    }
    let srv = unsafe { &mut *server };
    let Some(callback) = srv._rclc.cancel_callback else {
        return reject;
    };
    let context = srv._rclc.context;
    let goal_id = unsafe { (*goal).uuid };
    let Some(handle) = rclc_goal_handle_for(srv, &goal_id) else {
        return reject;
    };
    if handle.status != GOAL_STATE_EXECUTING {
        return reject;
    }
    // Raw from here, as in the goal trampoline.
    let handle: *mut rclc_action_goal_handle_t = handle;
    unsafe {
        (*handle).status = GOAL_STATE_CANCELING;
        if callback(handle, context) {
            (*handle).goal_cancelled = true;
            rclc_cancel_response_t::RCLC_CANCEL_ACCEPT
        } else {
            (*handle).status = GOAL_STATE_EXECUTING;
            reject
        }
    }
}

/// rclc's `rclc_action_server_init_default(action_server, node, support,
/// type_support, action_name)` — phase-515 C4.
///
/// Records the server's name and descriptor; the callbacks arrive at
/// [`rclc_executor_add_action_server`], as in rclc.
///
/// ADOPT-BOUNDED: `support` is checked to be initialised and otherwise unused.
/// rclc passes its clock to `rcl_action_server_init`; here the executor's
/// clock times the server, and no entity exists until registration.
///
/// **`support` may be NULL**, which rclc refuses — as for
/// `rclc_timer_init_default`: a C component the generated entry hosts has no
/// support to pass (`<rclc/detail/rcl_compat.h>` §4). Accepting NULL widens
/// rclc's domain without changing any call rclc accepts.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL argument (other than `support`) or an
///   empty name
/// * `RCL_RET_NOT_INIT` — a non-NULL `support`, or `node`, is not initialised
/// * `RCLC_RET_BAD_SEQUENCE` — `action_server` is already initialised
///
/// # Safety
/// All pointers must be valid; `type_support` must outlive the server.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_server_init_default(
    action_server: *mut rclc_action_server_t,
    node: *mut rcl_node_t,
    support: *mut crate::support::rclc_support_t,
    type_support: *const rclc_action_type_t,
    action_name: *const core::ffi::c_char,
) -> rcl_ret_t {
    validate_not_null!(action_server, node, type_support, action_name);
    if !support.is_null()
        && unsafe { (*support).state }
            != crate::support::rclc_support_state_t::RCLC_SUPPORT_STATE_INITIALIZED
    {
        return RCL_RET_NOT_INIT;
    }
    unsafe {
        action_server_init_impl(
            action_server,
            node,
            action_name,
            type_support,
            None,
            None,
            None,
            ptr::null_mut(),
        )
    }
}

/// rclc's `rclc_executor_add_action_server(executor, action_server,
/// handles_number, ros_goal_request, ros_goal_request_size, goal_callback,
/// cancel_callback, context)` — phase-515 C4.
///
/// Registers the server with the executor and binds rclc's callbacks. Each
/// incoming goal takes one of `handles_number` goal handles; its request is
/// deserialised into element `i` of `ros_goal_request` (an array of the
/// action's `<Action>_SendGoal_Request`, `ros_goal_request_size` bytes each)
/// and `goal_callback` decides it. An accepted goal is answered with
/// [`rclc_action_send_result`]; [`rclc_action_publish_feedback`] reports
/// progress; `cancel_callback` decides a cancel request.
///
/// ADOPT-BOUNDED: at most `NROS_MAX_CONCURRENT_GOALS` handles (the pool is in
/// the server; rclc allocates it); a goal arriving with every handle in use is
/// rejected (rclc leaves it queued); a goal that does not deserialise is
/// rejected. A goal is `GOAL_STATE_EXECUTING` as soon as it is accepted, so
/// `rclc_action_send_result` succeeds before the client asks for the result
/// (the runtime keeps it until then) where rclc returns
/// `RCLC_RET_ACTION_WAIT_RESULT_REQUEST`.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL argument or callback, `handles_number`
///   0 or above `NROS_MAX_CONCURRENT_GOALS`, or `ros_goal_request_size`
///   smaller than the action's `<Action>_SendGoal_Request`
/// * `RCL_RET_ERROR` — `ros_goal_request_size` is 0 (rclc's answer)
/// * `RCL_RET_UNSUPPORTED` — the descriptor carries no codec or envelope layout
///   (built from names alone, or by a generator older than codegen 21)
/// * `RCLC_RET_BAD_SEQUENCE` — the server already has callbacks bound
/// * otherwise as `rclc_executor_attach_action_server`
///
/// # Safety
/// `executor` and `action_server` must be initialised; `ros_goal_request` must
/// hold `handles_number` elements and outlive the registration.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn rclc_executor_add_action_server(
    executor: *mut crate::executor::rclc_executor_t,
    action_server: *mut rclc_action_server_t,
    handles_number: usize,
    ros_goal_request: *mut c_void,
    ros_goal_request_size: usize,
    goal_callback: rclc_action_server_handle_goal_callback_t,
    cancel_callback: rclc_action_server_handle_cancel_callback_t,
    context: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(executor, action_server, ros_goal_request);
    if goal_callback.is_none() || cancel_callback.is_none() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    if ros_goal_request_size == 0 {
        return RCL_RET_ERROR;
    }
    if handles_number == 0 || handles_number > crate::constants::NROS_MAX_CONCURRENT_GOALS {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let server = unsafe { &mut *action_server };
    if server.state != rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED {
        return RCL_RET_NOT_INIT;
    }
    if server.goal_callback.is_some() {
        return RCLC_RET_BAD_SEQUENCE;
    }
    let Some(type_info) = (unsafe { server.type_support.as_ref() }) else {
        return RCL_RET_UNSUPPORTED;
    };
    if type_info.send_goal_request_size == 0
        || type_info.goal_deserialize.is_none()
        || type_info.result_serialize.is_none()
        || type_info.feedback_serialize.is_none()
    {
        return RCL_RET_UNSUPPORTED;
    }
    if ros_goal_request_size < type_info.send_goal_request_size {
        return RCL_RET_INVALID_ARGUMENT;
    }

    server._rclc = rclc_action_server_rclc_state_t {
        goal_callback,
        cancel_callback,
        context,
        goal_requests: ros_goal_request as *mut u8,
        goal_request_size: ros_goal_request_size,
        handles_number,
        ..rclc_action_server_rclc_state_t::NONE
    };
    server.goal_callback = Some(rclc_goal_trampoline);
    server.cancel_callback = Some(rclc_cancel_trampoline);
    server.accepted_callback = None;
    server.context = ptr::null_mut();

    let ret =
        unsafe { crate::executor::rclc_executor_attach_action_server(executor, action_server) };
    if ret != RCL_RET_OK {
        let server = unsafe { &mut *action_server };
        server.goal_callback = None;
        server.cancel_callback = None;
        server._rclc = rclc_action_server_rclc_state_t::NONE;
    }
    ret
}

/// The live rclc goal handle `goal_handle` names, and its server's descriptor
/// — or `None` for a handle that is not one of its server's live goals. The
/// handle lives INSIDE the server, so the server is read through its pointer
/// and never borrowed beside it.
unsafe fn rclc_live_goal<'a>(
    goal_handle: *mut rclc_action_goal_handle_t,
) -> Option<(&'a mut rclc_action_goal_handle_t, &'a rclc_action_type_t)> {
    let server = unsafe { (*goal_handle).action_server };
    if server.is_null() {
        return None;
    }
    let pool =
        unsafe { ptr::addr_of!((*server)._rclc.goal_handles) } as *const rclc_action_goal_handle_t;
    let at = goal_handle as *const rclc_action_goal_handle_t;
    let end = unsafe { pool.add(crate::constants::NROS_MAX_CONCURRENT_GOALS) };
    if at < pool || at >= end {
        return None;
    }
    let type_info = unsafe { (*server).type_support.as_ref() }?;
    let handle = unsafe { &mut *goal_handle };
    handle._in_use.then_some((handle, type_info))
}

/// Serialise the part at `offset` of `envelope` with `serialize` into `buf`.
unsafe fn rclc_serialize_part(
    serialize: crate::publisher::rclc_message_serialize_fn_t,
    envelope: *mut c_void,
    offset: usize,
    buf: &mut [u8],
) -> Option<usize> {
    let serialize = serialize?;
    let part = unsafe { (envelope as *const u8).add(offset) } as *const c_void;
    let mut len = 0usize;
    let rc = unsafe { serialize(part, buf.as_mut_ptr(), buf.len(), &mut len) };
    (rc == 0 && len <= buf.len()).then_some(len)
}

/// rclc's `rclc_action_send_result(goal_handle, status, ros_response)` —
/// phase-515 C4.
///
/// Ends the goal with `status` (`GOAL_STATE_SUCCEEDED`, `_CANCELED` or
/// `_ABORTED`) and the result in `ros_response`, the action's
/// `<Action>_GetResult_Response`, whose `status` this writes, as rclc does. The
/// runtime keeps the result and answers the client's result request with it;
/// the goal handle is free again on return.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL argument, a non-terminal `status`, or
///   a handle that is not a live goal
/// * `RCLC_RET_ACTION_WAIT_RESULT_REQUEST` — the goal is neither executing nor
///   canceling
/// * `RCL_RET_ERROR` — the result did not serialise, or the runtime refused it
///
/// # Safety
/// `ros_response` must be the action's `<Action>_GetResult_Response`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_send_result(
    goal_handle: *mut rclc_action_goal_handle_t,
    status: rcl_action_goal_state_t,
    ros_response: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(goal_handle, ros_response);
    if status <= GOAL_STATE_CANCELING || status > GOAL_STATE_ABORTED {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let Some((handle, type_info)) = (unsafe { rclc_live_goal(goal_handle) }) else {
        return RCL_RET_INVALID_ARGUMENT;
    };
    if handle.status != GOAL_STATE_EXECUTING && handle.status != GOAL_STATE_CANCELING {
        return RCLC_RET_ACTION_WAIT_RESULT_REQUEST;
    }
    // `status` is the envelope's first member.
    unsafe { *(ros_response as *mut i8) = status };
    let mut buf = [0u8; crate::config::MESSAGE_BUFFER_SIZE];
    let Some(len) = (unsafe {
        rclc_serialize_part(
            type_info.result_serialize,
            ros_response,
            type_info.get_result_response_result_offset,
            &mut buf,
        )
    }) else {
        return RCL_RET_ERROR;
    };
    let goal = rclc_goal_handle_t {
        uuid: handle.goal_id,
    };
    let server_ptr = handle.action_server;
    // The handle lives inside the server the calls below borrow, so it is
    // written through its pointer afterwards rather than held across them.
    let ret = unsafe {
        match status {
            GOAL_STATE_SUCCEEDED => rclc_action_succeed(server_ptr, &goal, buf.as_ptr(), len),
            GOAL_STATE_CANCELED => rclc_action_canceled(server_ptr, &goal, buf.as_ptr(), len),
            _ => rclc_action_abort(server_ptr, &goal, buf.as_ptr(), len),
        }
    };
    if ret == RCL_RET_OK {
        unsafe {
            (*goal_handle).status = status;
            (*goal_handle)._in_use = false;
        }
    }
    ret
}

/// rclc's `rclc_action_publish_feedback(goal_handle, ros_feedback)` —
/// phase-515 C4.
///
/// Publishes `ros_feedback`, the action's `<Action>_FeedbackMessage`, for the
/// goal; its `goal_id` is written from the handle first, as rclc does.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` — a NULL argument or a handle that is not a
///   live goal
/// * `RCL_RET_ERROR` — the feedback did not serialise, or the publish failed
///
/// # Safety
/// `ros_feedback` must be the action's `<Action>_FeedbackMessage`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_publish_feedback(
    goal_handle: *mut rclc_action_goal_handle_t,
    ros_feedback: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(goal_handle, ros_feedback);
    let Some((handle, type_info)) = (unsafe { rclc_live_goal(goal_handle) }) else {
        return RCL_RET_INVALID_ARGUMENT;
    };
    // `goal_id` is the envelope's first member.
    unsafe { ptr::copy_nonoverlapping(handle.goal_id.uuid.as_ptr(), ros_feedback as *mut u8, 16) };
    let mut buf = [0u8; crate::config::MESSAGE_BUFFER_SIZE];
    let Some(len) = (unsafe {
        rclc_serialize_part(
            type_info.feedback_serialize,
            ros_feedback,
            type_info.feedback_message_feedback_offset,
            &mut buf,
        )
    }) else {
        return RCL_RET_ERROR;
    };
    let goal = rclc_goal_handle_t {
        uuid: handle.goal_id,
    };
    let server_ptr = handle.action_server;
    unsafe { rclc_action_publish_feedback_raw(server_ptr, &goal, buf.as_ptr(), len) }
}

/// rclc's `rclc_action_server_fini(action_server, node)` — phase-515 C4:
/// [`rcl_action_server_fini`] under rclc's name.
///
/// # Safety
/// As [`rcl_action_server_fini`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_server_fini(
    action_server: *mut rclc_action_server_t,
    node: *mut rcl_node_t,
) -> rcl_ret_t {
    unsafe { rcl_action_server_fini(action_server, node) }
}

// ============================================================================
// Phase 122.3.c.6.b — Layer-1 primitive entry points (caller polls)
// ============================================================================

/// Phase 122.3.c.6.b — initialize an L1 polling-mode action server.
///
/// Creates the 5 transport channels immediately and stores the
/// `ActionServerCore` inline in the `rclc_polling_action_server_t`. The caller drives the
/// goal lifecycle via:
/// * `rclc_polling_action_server_try_recv_goal_request_raw` — poll for new
///   goal requests (returns goal_id + sequence + payload).
/// * `rclc_polling_action_server_accept_goal_raw` / `_reject_goal_raw` —
///   reply to the send_goal RPC.
/// * `rclc_polling_action_server_publish_feedback_raw` — push feedback.
/// * `rclc_polling_action_server_complete_goal_raw` — terminate (SUCCEEDED /
///   ABORTED / CANCELED).
/// * `rclc_polling_action_server_try_recv_cancel_request_raw` /
///   `_send_cancel_reply_raw` — the cancel RPC (once
///   ~~`nros_polling_action_server_try_handle_cancel_raw`~~).
/// * `rclc_polling_action_server_try_handle_get_result_raw` — the get_result RPC.
///
/// # Safety
/// All pointers must be valid; `action_name` must be a valid
/// null-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_init(
    server: *mut rclc_polling_action_server_t,
    node: *const rcl_node_t,
    type_info: *const super::common::rclc_action_type_t,
    action_name: *const core::ffi::c_char,
) -> rcl_ret_t {
    validate_not_null!(server, node, type_info, action_name);

    let server_mut = &mut *server;
    let node_ref = &*node;
    let type_info_ref = &*type_info;

    validate_state!(
        server_mut,
        rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_UNINITIALIZED,
        RCLC_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED);

    // Read into locals: the polled struct keeps the core, not the
    // description it was created from.
    let mut name_buf = [0u8; MAX_ACTION_NAME_LEN];
    let name_len = crate::util::copy_cstr_into(action_name, &mut name_buf);
    if name_len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let mut type_buf = [0u8; MAX_TYPE_NAME_LEN];
    let type_len = crate::util::copy_cstr_into(type_info_ref.type_name, &mut type_buf);
    let mut hash_buf = [0u8; MAX_TYPE_HASH_LEN];
    let hash_len = crate::util::copy_cstr_into(type_info_ref.type_hash, &mut hash_buf);
    // Read only by the channel creation below, which a no-RMW build omits.
    #[cfg(not(feature = "rmw-cffi"))]
    let _ = (&type_buf, type_len, &hash_buf, hash_len);

    // Resolve the SOURCE action name to its wire name — see
    // `rclc_publisher_init_with_qos`. All five channel keyexprs are derived
    // from it below, so resolving once here moves the whole action.
    let _resolved_name = {
        let source = core::str::from_utf8_unchecked(&name_buf[..name_len]);
        match crate::node::resolve_entity_name_on_node(node_ref, source) {
            Some(r) => r,
            None => return RCL_RET_INVALID_ARGUMENT,
        }
    };

    #[cfg(feature = "rmw-cffi")]
    {
        // Phase 156 Sub-bug D — multi-Session dispatch (see
        // `rclc_publisher_init_default`).
        let (mut session, domain_id) = match crate::node::resolve_session_and_domain(node_ref) {
            Some(t) => t,
            None => return RCL_RET_NOT_INIT,
        };

        // RESOLVED name on the wire — see the resolution above.
        let action_str = _resolved_name.as_str();
        let type_str = core::str::from_utf8_unchecked(&type_buf[..type_len]);
        let type_hash_str = core::str::from_utf8_unchecked(&hash_buf[..hash_len]);
        // Issue 1913 — each endpoint on its own hash.
        let hashes = type_info_ref.endpoint_hashes.resolve(type_hash_str);
        let node_name_str = core::str::from_utf8_unchecked(&node_ref.name[..node_ref.name_len]);
        let namespace_str =
            core::str::from_utf8_unchecked(&node_ref.namespace[..node_ref.namespace_len]);

        // Inline the 5-channel build (mirror of
        // `Node::create_action_server_raw_sized` but operating on
        // a raw `&mut Session` to avoid creating a temporary
        // `Node`).
        use nros_node::{ActionInfo, QoSProfile, ServiceInfo, Session, TopicInfo};
        let action_info =
            ActionInfo::new(action_str, type_str, type_hash_str).with_domain(domain_id);

        // Issue 0454 / phase-354 W3 — the per-CHANNEL DDS type, not the bare
        // action type. The type name is baked into the keyexpr, so advertising
        // `…Fibonacci_` where ROS 2 expects `…Fibonacci_SendGoal_` makes the
        // client's query and the server's queryable different keys: every goal
        // times out. phase-338 W3 fixed this on the raw REGISTER path
        // (`action_channel_type`'s own doc records it); these polling arms are
        // the same defect, unfixed because nothing called them until the
        // raw-goal probe did.
        let send_goal_keyexpr: nros_core::heapless::String<256> = action_info.send_goal_key();
        let send_goal_type: nros_core::heapless::String<256> =
            nros_node::action_channel_type(type_str, "SendGoal");
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, &send_goal_type, hashes.send_goal)
                .with_domain(domain_id)
                .with_node_name(node_name_str)
                .with_namespace(namespace_str);
        let send_goal_server =
            match session.create_service(&send_goal_info, QoSProfile::services_default()) {
                Ok(h) => h,
                Err(_) => return RCL_RET_ERROR,
            };

        let cancel_goal_keyexpr: nros_core::heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            hashes.cancel_goal,
        )
        .with_domain(domain_id)
        .with_node_name(node_name_str)
        .with_namespace(namespace_str);
        let cancel_goal_server =
            match session.create_service(&cancel_goal_info, QoSProfile::services_default()) {
                Ok(h) => h,
                Err(_) => return RCL_RET_ERROR,
            };

        let get_result_keyexpr: nros_core::heapless::String<256> = action_info.get_result_key();
        let get_result_type: nros_core::heapless::String<256> =
            nros_node::action_channel_type(type_str, "GetResult");
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, &get_result_type, hashes.get_result)
                .with_domain(domain_id)
                .with_node_name(node_name_str)
                .with_namespace(namespace_str);
        let get_result_server =
            match session.create_service(&get_result_info, QoSProfile::services_default()) {
                Ok(h) => h,
                Err(_) => return RCL_RET_ERROR,
            };

        let feedback_keyexpr: nros_core::heapless::String<256> = action_info.feedback_key();
        let feedback_type: nros_core::heapless::String<256> =
            nros_node::action_channel_type(type_str, "FeedbackMessage");
        let feedback_topic = TopicInfo::new(&feedback_keyexpr, &feedback_type, hashes.feedback)
            .with_domain(domain_id)
            .with_node_name(node_name_str)
            .with_namespace(namespace_str);
        let feedback_publisher =
            match session.create_publisher(&feedback_topic, QoSProfile::BEST_EFFORT) {
                Ok(h) => h,
                Err(_) => return RCL_RET_ERROR,
            };

        let status_keyexpr: nros_core::heapless::String<256> = action_info.status_key();
        let status_topic = TopicInfo::new(
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            hashes.status,
        )
        .with_domain(domain_id)
        .with_node_name(node_name_str)
        .with_namespace(namespace_str);
        let status_publisher =
            match session.create_publisher(&status_topic, QoSProfile::BEST_EFFORT) {
                Ok(h) => h,
                Err(_) => return RCL_RET_ERROR,
            };

        type Core = nros_node::ActionServerCore<
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
            4,
        >;
        let core = Core::from_channels(
            send_goal_server,
            cancel_goal_server,
            get_result_server,
            feedback_publisher,
            status_publisher,
        );
        core::ptr::write(server_mut._opaque.as_mut_ptr() as *mut Core, core);
    }

    server_mut.node = unsafe { crate::node::node_ref_of(node) };
    server_mut.state = rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED;
    RCL_RET_OK
}

/// Finalize a polled action server: drops the inline core, which closes its
/// five channels.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if `server` is NULL
/// * `RCL_RET_NOT_INIT` if it is not initialised
/// * `RCLC_RET_STALE_NODE` if its node has already been finalised (issue 1386)
///
/// # Safety
/// `server` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_fini(
    server: *mut rclc_polling_action_server_t,
) -> rcl_ret_t {
    validate_not_null!(server);
    let server = &mut *server;
    if server.state != rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED {
        return RCL_RET_NOT_INIT;
    }
    if server.node.is_bound() && !crate::node::node_ref_is_live(server.node) {
        return crate::error::RCLC_RET_STALE_NODE;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        core::ptr::drop_in_place(server._opaque.as_mut_ptr() as *mut PollingServerCore);
        server._opaque = [0u64; crate::opaque_sizes::ACTION_SERVER_OPAQUE_U64S];
    }
    server.node = crate::node::rclc_node_ref_t::none();
    server.state = rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_SHUTDOWN;
    RCL_RET_OK
}

#[cfg(feature = "rmw-cffi")]
type PollingServerCore = nros_node::ActionServerCore<
    { crate::config::MESSAGE_BUFFER_SIZE },
    { crate::config::MESSAGE_BUFFER_SIZE },
    { crate::config::MESSAGE_BUFFER_SIZE },
    4,
>;

#[cfg(feature = "rmw-cffi")]
#[inline]
unsafe fn polling_server_core(
    server: *mut rclc_polling_action_server_t,
) -> Option<&'static mut PollingServerCore> {
    if server.is_null() {
        return None;
    }
    let server_mut = &mut *server;
    if server_mut.state != rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_INITIALIZED {
        return None;
    }
    Some(&mut *(server_mut._opaque.as_mut_ptr() as *mut PollingServerCore))
}

/// The polled twin of [`rclc_action_server_get_active_goal_count`].
///
/// # Returns
/// `RCL_RET_OK` with `*out` written, `RCL_RET_INVALID_ARGUMENT` for a NULL
/// argument, `RCL_RET_NOT_INIT` when the server is not initialised.
///
/// # Safety
/// `server` and `out` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_get_active_goal_count(
    server: *mut rclc_polling_action_server_t,
    out: *mut usize,
) -> rcl_ret_t {
    validate_not_null!(server, out);
    #[cfg(feature = "rmw-cffi")]
    {
        match polling_server_core(server) {
            Some(core) => {
                *out = core.active_goal_count();
                RCL_RET_OK
            }
            None => RCL_RET_NOT_INIT,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        RCL_RET_NOT_INIT
    }
}

/// The polled twin of [`rclc_action_server_goal_exists`].
///
/// # Safety
/// `server` and `goal` must be NULL or valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_goal_exists(
    server: *mut rclc_polling_action_server_t,
    goal: *const rclc_goal_handle_t,
) -> bool {
    if server.is_null() || goal.is_null() {
        return false;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let goal_id = nros_node::GoalUuid {
            uuid: (*goal).uuid.uuid,
        };
        match polling_server_core(server) {
            Some(core) => core.goal_exists(&goal_id),
            None => false,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        false
    }
}

/// The polled twin of [`rclc_action_expire_goals`].
///
/// # Safety
/// `server` and `num_expired` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_expire_goals(
    server: *mut rclc_polling_action_server_t,
    num_expired: *mut usize,
) -> rcl_ret_t {
    validate_not_null!(server, num_expired);
    #[cfg(feature = "rmw-cffi")]
    {
        match polling_server_core(server) {
            Some(core) => {
                *num_expired = core.expire_completed_results();
                RCL_RET_OK
            }
            None => RCL_RET_NOT_INIT,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: try to receive a goal request.
///
/// On success writes the goal payload bytes (already stripped of CDR
/// framing) into `buf`, returns the number of bytes copied (>= 0),
/// and fills `goal_id_out` (16 bytes) + `sequence_number_out`. Use
/// the sequence number with `rclc_polling_action_server_accept_goal_raw` /
/// `_reject_goal_raw`.
///
/// Returns `0` when no request is pending; negative `rcl_ret_t` on
/// error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_try_recv_goal_request_raw(
    server: *mut rclc_polling_action_server_t,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
    sequence_number_out: *mut i64,
) -> i32 {
    if server.is_null()
        || (buf.is_null() && buf_len != 0)
        || goal_id_out.is_null()
        || sequence_number_out.is_null()
    {
        return RCL_RET_INVALID_ARGUMENT;
    }

    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        match core.try_recv_goal_request() {
            Ok(Some(req)) => {
                // Goal payload sits in core.goal_buffer() at
                // [GOAL_REQUEST_PAYLOAD_OFFSET..+req.data_len] (issue 2014).
                let goal_buf = core.goal_buffer();
                let payload_offset = action_frame::GOAL_REQUEST_PAYLOAD_OFFSET;
                if payload_offset + req.data_len > goal_buf.len() {
                    return RCL_RET_ERROR;
                }
                let copy_len = req.data_len.min(buf_len);
                core::ptr::copy_nonoverlapping(
                    goal_buf.as_ptr().add(payload_offset),
                    buf,
                    copy_len,
                );
                (*goal_id_out).copy_from_slice(&req.goal_id.uuid);
                *sequence_number_out = req.sequence_number;
                copy_len as i32
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (buf, buf_len, goal_id_out, sequence_number_out);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: accept a goal received via
/// `rclc_polling_action_server_try_recv_goal_request_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_accept_goal_raw(
    server: *mut rclc_polling_action_server_t,
    goal_id: *const [u8; 16],
    sequence_number: i64,
) -> rcl_ret_t {
    if server.is_null() || goal_id.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let id = nros::GoalUuid { uuid: *goal_id };
        core.accept_goal(id, sequence_number)
            .map(|_| RCL_RET_OK)
            .unwrap_or(RCL_RET_ERROR)
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (goal_id, sequence_number);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: reject a goal received via
/// `rclc_polling_action_server_try_recv_goal_request_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_reject_goal_raw(
    server: *mut rclc_polling_action_server_t,
    sequence_number: i64,
) -> rcl_ret_t {
    if server.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        core.reject_goal(sequence_number)
            .map(|_| RCL_RET_OK)
            .unwrap_or(RCL_RET_ERROR)
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = sequence_number;
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: publish a feedback message.
///
/// `feedback_cdr` is the CDR-encoded `<Action>_Feedback_` payload
/// (without the goal_id prefix — the core wraps it).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_publish_feedback_raw(
    server: *mut rclc_polling_action_server_t,
    goal_id: *const [u8; 16],
    feedback_cdr: *const u8,
    feedback_len: usize,
) -> rcl_ret_t {
    if server.is_null() || goal_id.is_null() || (feedback_cdr.is_null() && feedback_len != 0) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let id = nros::GoalUuid { uuid: *goal_id };
        let slice = core::slice::from_raw_parts(feedback_cdr, feedback_len);
        core.publish_feedback_raw(&id, slice)
            .map(|_| RCL_RET_OK)
            .unwrap_or(RCL_RET_ERROR)
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (goal_id, feedback_cdr, feedback_len);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: mark a goal terminal.
///
/// `status` must be one of SUCCEEDED / ABORTED / CANCELED.
/// `result_cdr` is the CDR-encoded `<Action>_Result_` payload (the
/// core handles the status wrapper before storing in the slab).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_complete_goal_raw(
    server: *mut rclc_polling_action_server_t,
    goal_id: *const [u8; 16],
    status: rclc_goal_status_t,
    result_cdr: *const u8,
    result_len: usize,
) -> rcl_ret_t {
    if server.is_null() || goal_id.is_null() || (result_cdr.is_null() && result_len != 0) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let id = nros::GoalUuid { uuid: *goal_id };
        let rust_status = c_status_to_rust(status);
        let slice = core::slice::from_raw_parts(result_cdr, result_len);
        // issue 0796 — see `rclc_action_succeed`.
        match core.complete_goal_raw(&id, rust_status, slice) {
            Ok(()) => RCL_RET_OK,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (goal_id, status, result_cdr, result_len);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.d — L1 polling: peek a pending cancel-goal
/// request. Writes the named goal_id, the matching service
/// sequence number, and the goal's current status (matches the
/// `rclc_goal_status_t` discriminants — pass it back unchanged).
///
/// Returns `1` when a request was peeked, `0` when none pending,
/// negative `rcl_ret_t` on error.
///
/// After a successful peek, call
/// `rclc_polling_action_server_send_cancel_reply_raw` with the recorded
/// `sequence_number` and the list of goals to accept-cancel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_try_recv_cancel_request_raw(
    server: *mut rclc_polling_action_server_t,
    goal_id_out: *mut [u8; 16],
    sequence_number_out: *mut i64,
    current_status_out: *mut rclc_goal_status_t,
) -> i32 {
    if server.is_null()
        || goal_id_out.is_null()
        || sequence_number_out.is_null()
        || current_status_out.is_null()
    {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        match core.try_recv_cancel_request() {
            Ok(Some(req)) => {
                (*goal_id_out).copy_from_slice(&req.goal_id.uuid);
                *sequence_number_out = req.sequence_number;
                *current_status_out = goal_status_from_core(req.current_status);
                1
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (goal_id_out, sequence_number_out, current_status_out);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.d — overall cancel-RPC return code. Distinct from
/// the per-goal `rclc_cancel_response_t` (ACCEPT/REJECT) used by the
/// L2 callback path. These four discriminants mirror
/// `nros_core::CancelResponseCode` and the `action_msgs/srv/CancelGoal`
/// wire-CDR `return_code` field. (Issue 0796 — the Rust type was called
/// `CancelResponse` until it was split; this pair of C names is what the
/// Rust names now follow.)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_cancel_return_code_t {
    RCLC_CANCEL_RC_OK = 0,
    RCLC_CANCEL_RC_REJECTED = 1,
    RCLC_CANCEL_RC_UNKNOWN_GOAL = 2,
    RCLC_CANCEL_RC_GOAL_TERMINATED = 3,
}

/// Phase 122.3.c.6.d — L1 polling: reply to a cancel-goal request
/// previously peeked via `_try_recv_cancel_request_raw`. `accepted`
/// points to `accepted_count` goal-ID byte arrays that will
/// transition to `CANCELING`. Pass an empty list (`accepted=NULL,
/// accepted_count=0`) with
/// `return_code=RCLC_CANCEL_RC_REJECTED` to refuse the request
/// entirely.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_send_cancel_reply_raw(
    server: *mut rclc_polling_action_server_t,
    sequence_number: i64,
    return_code: rclc_cancel_return_code_t,
    accepted: *const [u8; 16],
    accepted_count: usize,
) -> rcl_ret_t {
    if server.is_null() || (accepted.is_null() && accepted_count != 0) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let cancel_resp = match return_code {
            rclc_cancel_return_code_t::RCLC_CANCEL_RC_OK => nros_node::CancelResponseCode::Ok,
            rclc_cancel_return_code_t::RCLC_CANCEL_RC_REJECTED => {
                nros_node::CancelResponseCode::Rejected
            }
            rclc_cancel_return_code_t::RCLC_CANCEL_RC_UNKNOWN_GOAL => {
                nros_node::CancelResponseCode::UnknownGoal
            }
            rclc_cancel_return_code_t::RCLC_CANCEL_RC_GOAL_TERMINATED => {
                nros_node::CancelResponseCode::GoalTerminated
            }
        };
        // Build a stack-resident slice of GoalIds from the caller's
        // contiguous byte array. Cap at 8 — the wire format allows
        // more but no current backend handles more than a few
        // simultaneous cancels.
        let mut ids: nros_core::heapless::Vec<nros::GoalUuid, 8> = nros_core::heapless::Vec::new();
        for i in 0..accepted_count {
            if i >= 8 {
                return RCLC_RET_BAD_SEQUENCE;
            }
            let uuid = *accepted.add(i);
            if ids.push(nros::GoalUuid { uuid }).is_err() {
                return RCL_RET_ERROR;
            }
        }
        match core.send_cancel_reply(sequence_number, cancel_resp, &ids) {
            Ok(()) => RCL_RET_OK,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (sequence_number, return_code, accepted, accepted_count);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: serve a pending get_result query.
///
/// `default_result_cdr` is the default serialized result (without
/// status byte / CDR header) returned to the client when no
/// `complete_goal_raw` has been called for the queried goal yet.
///
/// Returns `0` when no query is pending; `1` when one was served;
/// negative `rcl_ret_t` on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_try_handle_get_result_raw(
    server: *mut rclc_polling_action_server_t,
    default_result_cdr: *const u8,
    default_result_len: usize,
) -> i32 {
    if server.is_null() || (default_result_cdr.is_null() && default_result_len != 0) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let slice = core::slice::from_raw_parts(default_result_cdr, default_result_len);
        match core.try_handle_get_result_raw(slice) {
            Ok(Some(_)) => 1,
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (default_result_cdr, default_result_len);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.e — register a C wake callback on the
/// send_goal channel of an L1 polling-mode action server. `state`
/// is a caller-owned `rclc_wake_state_t` that must outlive the
/// server and not move.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_set_goal_wake_callback(
    server: *mut rclc_polling_action_server_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    set_action_server_wake_callback(server, state, cb, ctx, ChannelKind::Goal)
}

/// Phase 122.3.c.6.e — register a C wake callback on the
/// cancel_goal channel of an L1 polling-mode action server. The
/// primary use case from the .c.6.e design discussion — RTOS /
/// event-driven cancel handling.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_set_cancel_wake_callback(
    server: *mut rclc_polling_action_server_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    set_action_server_wake_callback(server, state, cb, ctx, ChannelKind::Cancel)
}

/// Phase 122.3.c.6.e — register a C wake callback on the
/// get_result channel of an L1 polling-mode action server.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_server_set_get_result_wake_callback(
    server: *mut rclc_polling_action_server_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    set_action_server_wake_callback(server, state, cb, ctx, ChannelKind::GetResult)
}

enum ChannelKind {
    Goal,
    Cancel,
    GetResult,
}

unsafe fn set_action_server_wake_callback(
    server: *mut rclc_polling_action_server_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
    kind: ChannelKind,
) -> rcl_ret_t {
    if server.is_null() || state.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_server_core(server) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let state_ptr = state as *mut nros_node::c_waker::CWakeState;
        core::ptr::write(
            state_ptr,
            nros_node::c_waker::CWakeState { fn_ptr: cb, ctx },
        );
        let waker = nros_node::c_waker::make_waker(state_ptr);
        let armed = match kind {
            ChannelKind::Goal => core.register_goal_waker(&waker),
            ChannelKind::Cancel => core.register_cancel_waker(&waker),
            ChannelKind::GetResult => core.register_get_result_waker(&waker),
        };
        // Issue 1940 — a backend with no per-entity wake says so.
        if armed {
            RCL_RET_OK
        } else {
            RCL_RET_UNSUPPORTED
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (state, cb, ctx, kind);
        RCL_RET_NOT_INIT
    }
}

// ~~`nros_action_server_active_goal_count_raw`~~ lived here until
// phase-379 W6 decision 3 merged it into
// `rclc_action_server_get_active_goal_count`, which now branches on
// `state` to serve the polling tier too. The old spelling survived one
// release as a deprecated `static inline` forwarder in `rclc/action.h` and was
// retired by phase-417 stage 6 step B; either way it exports no symbol from
// here.

// ============================================================================
// Kani Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;
    use core::ptr;

    // Helper to create a dummy action type info
    fn dummy_action_type() -> rclc_action_type_t {
        let type_name = b"example_interfaces::action::dds_::Fibonacci_\0";
        let type_hash = b"RIHS01_test\0";
        rclc_action_type_t {
            type_name: type_name.as_ptr() as *const core::ffi::c_char,
            type_hash: type_hash.as_ptr() as *const core::ffi::c_char,
            goal_serialized_size_max: 8,
            result_serialized_size_max: 264,
            feedback_serialized_size_max: 264,
            goal_serialize: None,
            goal_deserialize: None,
            result_serialize: None,
            result_deserialize: None,
            feedback_serialize: None,
            feedback_deserialize: None,
            endpoint_hashes: crate::action::common::rclc_action_endpoint_hashes_t::NULL,
            send_goal_request_size: 0,
            send_goal_request_goal_offset: 0,
            get_result_response_size: 0,
            get_result_response_result_offset: 0,
            feedback_message_size: 0,
            feedback_message_feedback_offset: 0,
        }
    }

    // Helper goal callback
    unsafe extern "C" fn dummy_goal_callback(
        _server: *mut rcl_action_server_t,
        _goal: *const rclc_goal_handle_t,
        _req: *const u8,
        _len: usize,
        _ctx: *mut core::ffi::c_void,
    ) -> rclc_goal_response_t {
        rclc_goal_response_t::RCLC_GOAL_ACCEPT_AND_EXECUTE
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_init_null_ptrs() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::rcl_get_zero_initialized_node();

        // NULL server
        assert_eq!(
            unsafe {
                rclc_action_server_init_with_callbacks(
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
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL node
        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe {
                rclc_action_server_init_with_callbacks(
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
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL action_name
        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe {
                rclc_action_server_init_with_callbacks(
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
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL type_info
        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe {
                rclc_action_server_init_with_callbacks(
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
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_init_none_goal_callback() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::rcl_get_zero_initialized_node();

        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe {
                rclc_action_server_init_with_callbacks(
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
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_init_uninit_node() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::rcl_get_zero_initialized_node();

        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe {
                rclc_action_server_init_with_callbacks(
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
            RCL_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_zero_initialized_state() {
        let srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            srv.state,
            rclc_action_server_state_t::RCLC_ACTION_SERVER_STATE_UNINITIALIZED,
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
            unsafe {
                rcl_action_server_fini(
                    ptr::null_mut(),
                    &mut crate::node::rcl_get_zero_initialized_node(),
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // UNINITIALIZED → NOT_INIT
        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe {
                rcl_action_server_fini(&mut srv, &mut crate::node::rcl_get_zero_initialized_node())
            },
            RCL_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_double_init_rejected() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let mut node = crate::node::rcl_get_zero_initialized_node();
        node.state = crate::node::rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED;

        let mut srv = rcl_action_get_zero_initialized_server();
        let ret = unsafe {
            rclc_action_server_init_with_callbacks(
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
        assert_eq!(ret, RCL_RET_OK);

        // Second init → BAD_SEQUENCE
        assert_eq!(
            unsafe {
                rclc_action_server_init_with_callbacks(
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
            RCLC_RET_BAD_SEQUENCE,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_server_active_goal_count_null() {
        let mut count: usize = 7;

        // NULL server.
        assert_eq!(
            unsafe { rclc_action_server_get_active_goal_count(ptr::null_mut(), &mut count) },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL out-param.
        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe { rclc_action_server_get_active_goal_count(&mut srv, ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT,
        );

        // Zero-initialised server is UNINITIALIZED: neither tier claims it.
        assert_eq!(
            unsafe { rclc_action_server_get_active_goal_count(&mut srv, &mut count) },
            RCL_RET_NOT_INIT,
        );

        // `*out` is untouched on every error path.
        assert_eq!(count, 7);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_publish_feedback_null_ptrs() {
        let feedback = [0u8; 8];
        let goal = rclc_goal_handle_t::default();

        // NULL server
        assert_eq!(
            unsafe {
                rclc_action_publish_feedback_raw(ptr::null_mut(), &goal, feedback.as_ptr(), 8)
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL goal
        let mut srv = rcl_action_get_zero_initialized_server();
        assert_eq!(
            unsafe {
                rclc_action_publish_feedback_raw(&mut srv, ptr::null(), feedback.as_ptr(), 8)
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL feedback
        assert_eq!(
            unsafe { rclc_action_publish_feedback_raw(&mut srv, &goal, ptr::null(), 0) },
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_succeed_null_ptr() {
        assert_eq!(
            unsafe { rclc_action_succeed(ptr::null_mut(), ptr::null(), ptr::null(), 0) },
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_abort_null_ptr() {
        assert_eq!(
            unsafe { rclc_action_abort(ptr::null_mut(), ptr::null(), ptr::null(), 0) },
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_canceled_null_ptr() {
        assert_eq!(
            unsafe { rclc_action_canceled(ptr::null_mut(), ptr::null(), ptr::null(), 0) },
            RCL_RET_INVALID_ARGUMENT,
        );
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// The name accessor and the two predicates answer HONESTLY on a server
    /// that was never initialised — phase-417 W4.b.
    ///
    /// Each of the three has an error channel that is easy to get wrong in the
    /// same direction: a name accessor that returned the zeroed `action_name`
    /// buffer would hand back an empty C string that reads like a real name,
    /// and a predicate that returned `true` on an uninitialised server would
    /// make the ported guard let the call through. rcl's accessor answers NULL
    /// for a handle that is not usable (`rcl_service_get_service_name`'s
    /// contract, which the C surface's other three name accessors already
    /// follow), and a goal nobody accepted does not exist.
    #[test]
    fn accessors_refuse_an_unusable_server() {
        let mut srv = rcl_action_get_zero_initialized_server();
        let goal = rclc_goal_handle_t {
            uuid: rclc_goal_uuid_t { uuid: [7u8; 16] },
        };
        let mut expired: usize = 12345;

        // NULL handle.
        assert!(unsafe { rcl_action_server_get_action_name(ptr::null()) }.is_null());
        assert!(!unsafe { rclc_action_server_goal_exists(ptr::null_mut(), &goal) });
        assert_eq!(
            unsafe { rclc_action_expire_goals(ptr::null_mut(), &mut expired) },
            RCL_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { rclc_action_expire_goals(&mut srv, ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT
        );

        // UNINITIALIZED handle — NOT an empty string, and NOT `true`.
        assert!(unsafe { rcl_action_server_get_action_name(&srv) }.is_null());
        assert!(!unsafe { rclc_action_server_goal_exists(&mut srv, &goal) });
        assert!(!unsafe { rclc_action_server_goal_exists(&mut srv, ptr::null()) });
        assert_eq!(
            unsafe { rclc_action_expire_goals(&mut srv, &mut expired) },
            RCL_RET_NOT_INIT
        );

        // The out-parameter is untouched on every error path, so a caller that
        // ignores the return code reads back its own initialiser rather than a
        // fabricated 0 — the same contract
        // `rclc_action_server_get_active_goal_count` states.
        assert_eq!(expired, 12345);
    }
}
