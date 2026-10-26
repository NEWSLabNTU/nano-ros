//! Action client implementation.

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
    executor::ffi::{ActionClientCoreFfi as _, ExecutorFfi as _},
};

/// Issue 2014 — byte of a get_result reply where the result fields start
/// (`[encap 4][status i8][pad → 8][fields]`), for a C caller decoding what
/// `rclc_polling_action_client_try_recv_result_raw` returns. C ABI mirror of
/// `nros_node::action_frame::RESULT_PAYLOAD_OFFSET` (a literal, because
/// cbindgen emits only literals — the assert below binds the two).
pub const RCLC_ACTION_RESULT_PAYLOAD_OFFSET: usize = 8;
/// Issue 2014 — byte of a feedback message where the feedback fields start
/// (`[encap 4][GoalUuid.uuid 16][fields]`), for a caller of
/// `rclc_polling_action_client_try_recv_feedback_raw`; mirror of
/// `nros_node::action_frame::FEEDBACK_PAYLOAD_OFFSET`.
pub const RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET: usize = 20;
const _: () = assert!(RCLC_ACTION_RESULT_PAYLOAD_OFFSET == action_frame::RESULT_PAYLOAD_OFFSET);
const _: () = assert!(RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET == action_frame::FEEDBACK_PAYLOAD_OFFSET);

// ============================================================================
// Internal implementation
// ============================================================================

/// Internal state for the action client.
///
/// Lightweight — stores only the arena entry index and executor pointer.
/// The `ActionClientCore` (transport handles) lives in the executor's arena,
/// created by `rclc_executor_attach_action_client`.
#[repr(C)]
pub struct ActionClientInternal {
    /// Arena entry index (set by rclc_executor_attach_action_client).
    /// -1 means not registered with executor.
    pub arena_entry_index: i32,
    /// Pointer to the executor (set by rclc_executor_attach_action_client).
    pub executor_ptr: *mut core::ffi::c_void,
}

impl ActionClientInternal {
    pub const fn new() -> Self {
        Self {
            arena_entry_index: -1,
            executor_ptr: core::ptr::null_mut(),
        }
    }

    /// Get a mutable reference to the ActionClientCore in the executor arena.
    ///
    /// Returns `None` if not yet registered with the executor.
    ///
    /// # Safety
    /// `executor_ptr` must point to a valid `CExecutor`.
    unsafe fn arena_core_mut(&mut self) -> Option<&mut nros_node::ActionClientCore> {
        if self.arena_entry_index < 0 || self.executor_ptr.is_null() {
            return None;
        }
        let exec = &mut *(self.executor_ptr as *mut crate::executor::CExecutor);
        unsafe { exec.action_client_core_mut(self.arena_entry_index as usize) }
    }
}

impl Default for ActionClientInternal {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Action Client
// ============================================================================

/// Action client state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_action_client_state_t {
    /// Not initialized
    RCLC_ACTION_CLIENT_STATE_UNINITIALIZED = 0,
    /// Initialized. For a `rcl_action_client_t` the transport is created
    /// by `rclc_executor_attach_action_client`; for a
    /// `rclc_polling_action_client_t` its `ActionClientCore` is live in the
    /// struct.
    RCLC_ACTION_CLIENT_STATE_INITIALIZED = 1,
    /// Shutdown
    RCLC_ACTION_CLIENT_STATE_SHUTDOWN = 2,
}

/// An action client the executor dispatches.
///
/// phase-487 W4 (RFC-0096 D9, the C half): the executor arena owns the
/// channels and buffers; this struct holds the arena handle (`_internal`),
/// the registration description and the callback record the arena's
/// trampolines read, as the C++ action client does. A client the caller
/// drives itself is a [`rclc_polling_action_client_t`].
#[repr(C)]
pub struct rcl_action_client_t {
    /// Current state
    pub state: rclc_action_client_state_t,
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
    /// Goal response callback (for async send_goal)
    pub goal_response_callback: rclc_goal_response_callback_t,
    /// Feedback callback
    pub feedback_callback: rclc_feedback_callback_t,
    /// Result callback
    pub result_callback: rclc_result_callback_t,
    /// Issue 2030 — cancel-response callback (optional; see
    /// [`rclc_action_client_set_cancel_response_callback`]).
    pub cancel_response_callback: rclc_cancel_response_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: crate::node::rclc_node_ref_t,
    /// Phase 189.M3.3.b — scheduling-context slot to bind the action client's
    /// executor handle to. `0` = inherit the executor / Node default; set via
    /// `rcl_action_client_init`'s `options`. When non-zero,
    /// `rclc_executor_attach_action_client` binds the handle after
    /// registration. No effect on the L1 polling path.
    pub sched_context_id: crate::executor::rclc_sched_context_id_t,
    /// Internal state (arena entry index + executor pointer). Phase 87.5:
    /// Typed C-ABI handle field.
    pub _internal: ActionClientInternal,
}

/// An action client the CALLER drives, with no executor (phase-487 W4).
///
/// The C counterpart of C++'s `nros::PollingActionClient<A>`. `_opaque` holds
/// an `ActionClientCore` from `rclc_polling_action_client_init` until
/// `rclc_polling_action_client_fini`. Zero-initialised is UNINITIALIZED.
/// Until phase-487 it was the `NROS_ACTION_CLIENT_STATE_POLLING` state of
/// `rcl_action_client_t`.
#[repr(C)]
pub struct rclc_polling_action_client_t {
    /// Current state: UNINITIALIZED, INITIALIZED (core live) or SHUTDOWN.
    pub state: rclc_action_client_state_t,
    /// The node the channels were created on.
    pub node: crate::node::rclc_node_ref_t,
    /// Inline storage for the `ActionClientCore`. Internal: treat as opaque.
    pub _opaque: [u64; crate::opaque_sizes::ACTION_CLIENT_OPAQUE_U64S],
}

impl Default for rclc_polling_action_client_t {
    // The `_opaque` array sizes to the probed ActionClient size, which
    // exceeds clippy's 16 KB stack-array threshold. It is RVO'd into its
    // destination by the optimiser.
    #[allow(clippy::large_stack_arrays)]
    fn default() -> Self {
        Self {
            state: rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_UNINITIALIZED,
            node: crate::node::rclc_node_ref_t::none(),
            _opaque: [0u64; crate::opaque_sizes::ACTION_CLIENT_OPAQUE_U64S],
        }
    }
}

/// Get a zero-initialized polling action client.
#[unsafe(no_mangle)]
#[allow(clippy::large_stack_arrays)]
pub extern "C" fn rclc_get_zero_initialized_polling_action_client() -> rclc_polling_action_client_t
{
    rclc_polling_action_client_t::default()
}

impl Default for rcl_action_client_t {
    fn default() -> Self {
        Self {
            state: rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_UNINITIALIZED,
            action_name: [0u8; MAX_ACTION_NAME_LEN],
            action_name_len: 0,
            type_name: [0u8; MAX_TYPE_NAME_LEN],
            type_name_len: 0,
            type_hash: [0u8; MAX_TYPE_HASH_LEN],
            type_hash_len: 0,
            endpoint_hashes: crate::action::common::rclc_action_endpoint_hashes_t::NULL,
            goal_response_callback: None,
            feedback_callback: None,
            result_callback: None,
            cancel_response_callback: None,
            context: ptr::null_mut(),
            node: crate::node::rclc_node_ref_t::none(),
            sched_context_id: 0,
            _internal: ActionClientInternal::new(),
        }
    }
}

// ============================================================================
// Action Client Functions
// ============================================================================

/// Get a zero-initialized action client.
#[unsafe(no_mangle)]
pub extern "C" fn rcl_action_get_zero_initialized_client() -> rcl_action_client_t {
    rcl_action_client_t::default()
}

/// Initialize an action client — rcl's signature (phase-492 W1.b):
/// `rcl_action_client_init(&client, &node, ROSIDL_GET_ACTION_TYPE_SUPPORT(...),
/// "name", &options)`. The type support is our descriptor
/// (`ROSIDL_GET_ACTION_TYPE_SUPPORT` yields its address, RFC-0089 2026-10-12).
/// `options` may be NULL for the defaults; a non-zero `sched_context` is
/// applied by `rclc_executor_attach_action_client` once the handle exists.
///
/// # Safety
/// All non-NULL pointers must be valid and the node initialized; `options`
/// may be NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_action_client_init(
    client: *mut rcl_action_client_t,
    node: *const rcl_node_t,
    type_info: *const rclc_action_type_t,
    action_name: *const core::ffi::c_char,
    options: *const rcl_action_client_options_t,
) -> rcl_ret_t {
    validate_not_null!(client, node, action_name, type_info);

    let client = &mut *client;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        client,
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_UNINITIALIZED,
        RCLC_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED);

    // Copy action name (required — empty rejected)
    client.action_name_len = crate::util::copy_cstr_into(action_name, &mut client.action_name);
    if client.action_name_len == 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    client.type_name_len = crate::util::copy_cstr_into(type_info.type_name, &mut client.type_name);
    client.type_hash_len = crate::util::copy_cstr_into(type_info.type_hash, &mut client.type_hash);
    client.endpoint_hashes = type_info.endpoint_hashes;

    // Store node pointer
    client.node = unsafe { crate::node::node_ref_of(node) };

    // Metadata only — no transport handles created here.
    // Transport handles are created in rclc_executor_attach_action_client,
    // which places the ActionClientCore in the executor's arena.
    client._internal = ActionClientInternal::new();

    if !options.is_null() {
        client.sched_context_id = (*options).sched_context;
    }

    client.state = rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED;

    RCL_RET_OK
}

/// Phase 189.M3.3.b — rclc-style named action-client options (action clients
/// carry no QoS field, so this is options-only). Zero-init = default behaviour.
#[repr(C)]
#[derive(Default)]
pub struct rcl_action_client_options_t {
    /// Scheduling-context slot to bind the action client's executor handle to.
    /// `0` = inherit the executor / Node default. A non-zero value must be an id
    /// from `rclc_executor_create_sched_context`; the bind is applied by
    /// `rclc_executor_attach_action_client` once the handle exists. No effect
    /// on the L1 polling path.
    pub sched_context: crate::executor::rclc_sched_context_id_t,
    /// Reserved for future use; must be zero. Pads for ABI stability.
    pub _reserved: [u8; 3],
}

/// Get a zero-initialised [`rcl_action_client_options_t`] (`sched_context = 0`).
#[unsafe(no_mangle)]
pub extern "C" fn rcl_action_client_get_default_options() -> rcl_action_client_options_t {
    rcl_action_client_options_t::default()
}

/// Set feedback callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_client_set_feedback_callback(
    client: *mut rcl_action_client_t,
    callback: rclc_feedback_callback_t,
    context: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(client);

    let client = &mut *client;
    client.feedback_callback = callback;
    client.context = context;

    RCL_RET_OK
}

/// Set result callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_client_set_result_callback(
    client: *mut rcl_action_client_t,
    callback: rclc_result_callback_t,
    context: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(client);

    let client = &mut *client;
    client.result_callback = callback;
    client.context = context;

    RCL_RET_OK
}

/// Set the cancel-response callback — issue 2030.
///
/// Called during `rclc_executor_spin_some` (or `rclc_action_client_poll`) when
/// the server answers a `rclc_action_cancel_goal`. The executor takes cancel
/// replies whether or not a callback is set: each cancel holds one of the
/// client's request slots until its reply is taken.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_client_set_cancel_response_callback(
    client: *mut rcl_action_client_t,
    callback: rclc_cancel_response_callback_t,
    context: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(client);

    let client = &mut *client;
    client.cancel_response_callback = callback;
    client.context = context;

    RCL_RET_OK
}

/// Block until the action server's send-goal queryable is discoverable
/// on the network, or `timeout_ms` elapses.
///
/// Mirrors `rclcpp_action::Client::wait_for_action_server` and the
/// underlying `ActionClient::wait_for_action_server`. Spins on the
/// action's `send_goal` service client's readiness (the goal queryable is
/// the load-bearing entity for the first `rclc_action_send_goal` call) via
/// the same primitive as `rclc_client_wait_for_service` (phase-428 W13).
///
/// # Returns
/// * `RCL_RET_OK` — server visible.
/// * `RCL_RET_TIMEOUT` — `timeout_ms` elapsed without seeing a token.
/// * `RCL_RET_UNSUPPORTED` — the backend cannot know whether a server is up
///   (XRCE: the Agent owns the DDS graph), returned at once rather than after
///   the budget, because no answer can arrive (issue 1686). A caller that may
///   send blind proceeds; the goal request's own timeout is then the probe.
/// * `RCL_RET_NOT_INIT` — client not registered with an executor.
/// * `RCL_RET_ERROR` — transport-level failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_client_wait_for_action_server(
    client: *mut rcl_action_client_t,
    executor: *mut crate::executor::rclc_executor_t,
    timeout_ms: u32,
) -> rcl_ret_t {
    validate_not_null!(client, executor);

    #[cfg(feature = "rmw-cffi")]
    {
        let client_ref = &mut *client;
        if client_ref.state != rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED {
            return RCL_RET_NOT_INIT;
        }
        let internal = &mut client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return RCL_RET_NOT_INIT;
        }
        // Note: action clients store an opaque pointer into
        // `executor._opaque` (see `rclc_executor_attach_action_client`),
        // not to the outer `rclc_executor_t`, so we can't recover the
        // wrapper from `internal.executor_ptr`. Take `executor` as a
        // separate argument — same convention as
        // `rclc_action_send_goal`.
        let exec_t = &mut *executor;
        if exec_t.in_dispatch {
            return RCLC_RET_REENTRANT;
        }

        // phase-428 W13 — spin, then ask again; see
        // `rclc_client_wait_for_service`.
        let start_ns = crate::platform::get_time_ns();
        let timeout_ns: u64 = (timeout_ms as u64).saturating_mul(1_000_000);
        loop {
            {
                let mut exec = crate::executor::get_executor(exec_t);
                let core = match exec.action_client_core_mut(internal.arena_entry_index as usize) {
                    Some(c) => c,
                    None => return RCL_RET_NOT_INIT,
                };
                // issue 1686 — one classification for every wait loop.
                match nros_node::executor::ServerVisibility::of(core.server_readiness()) {
                    nros_node::executor::ServerVisibility::Visible => return RCL_RET_OK,
                    nros_node::executor::ServerVisibility::Unknowable => {
                        return RCL_RET_UNSUPPORTED;
                    }
                    nros_node::executor::ServerVisibility::NotYet => {}
                }
            }
            let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
            if elapsed_ns >= timeout_ns {
                return RCL_RET_TIMEOUT;
            }
            crate::executor::rclc_executor_spin_some(executor, 10_000_000);
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (client, executor, timeout_ms);
        RCL_RET_OK
    }
}

/// Read back the action name this client was created on.
///
/// rcl's `rcl_action_client_get_action_name`, and the third of the C
/// surface's name accessors — `rcl_service_get_service_name`,
/// `rcl_subscription_get_topic_name` and `rcl_publisher_get_topic_name`
/// already had this shape, and the action client and server were the two
/// entities that did not (phase-417 W4.b, ledger row
/// `c:action_client_get_action_name`). The name is the caller's own string,
/// copied NUL-terminated into the handle at init, so this hands back a
/// pointer into the handle and costs nothing.
///
/// # Parameters
/// * `client` - Pointer to an action client
///
/// # Returns
/// * Pointer to the action name (NUL-terminated), or NULL when `client` is
///   NULL or was never initialised. A polled client is its own type since
///   phase-487 W4 (`rclc_polling_action_client_t`) and keeps no name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_action_client_get_action_name(
    client: *const rcl_action_client_t,
) -> *const core::ffi::c_char {
    if client.is_null() {
        return core::ptr::null();
    }
    let client = &*client;
    match client.state {
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED => {
            client.action_name.as_ptr() as *const core::ffi::c_char
        }
        _ => core::ptr::null(),
    }
}

/// Non-blocking snapshot of action-server visibility. Mirrors
/// `rclcpp_action::Client::action_server_is_ready`. Takes `executor`
/// for the same reason as
/// [`rclc_action_client_wait_for_action_server`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_client_action_server_is_ready(
    client: *const rcl_action_client_t,
    executor: *mut crate::executor::rclc_executor_t,
) -> bool {
    if client.is_null() || executor.is_null() {
        return false;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let client_ref = &*client;
        if client_ref.state != rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED {
            return false;
        }
        let internal = &client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return false;
        }
        let exec_t = &mut *executor;
        let mut exec = crate::executor::get_executor(exec_t);
        match exec.action_client_core_mut(internal.arena_entry_index as usize) {
            Some(core) => core.is_server_ready(),
            None => false,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (client, executor);
        true
    }
}

/// Send a goal request.
#[unsafe(no_mangle)]
/// Send a goal (blocking convenience).
///
/// Calls `rclc_action_send_goal_async` then spins the executor until the
/// goal is accepted/rejected or timeout. Never calls `zpico_get` directly —
/// all I/O is driven by the executor's `spin_once`.
///
/// Like the runtime's `Promise::wait`, this is syntactic sugar over async + spin.
///
/// # Returns
///
/// Three outcomes, three codes — they were two before issue 0868 (see the C++
/// twin `nros_cpp_action_client_send_goal`):
///
/// * `RCL_RET_OK` — the server ACCEPTED; `goal_uuid` is filled.
/// * `RCLC_RET_REJECTED` — the server RECEIVED the goal and declined it.
/// * `RCL_RET_TIMEOUT` — no goal response within the blocking budget. The
///   server may never have received the goal at all.
/// * `RCL_RET_ERROR` — the goal could not be SENT.
/// * `RCLC_RET_REENTRANT` — called from inside a dispatch callback.
#[allow(static_mut_refs)]
pub unsafe extern "C" fn rclc_action_send_goal(
    client: *mut rcl_action_client_t,
    executor: *mut crate::executor::rclc_executor_t,
    goal: *const u8,
    goal_len: usize,
    goal_uuid: *mut rclc_goal_uuid_t,
) -> rcl_ret_t {
    validate_not_null!(client, goal, goal_uuid, executor);

    // Reentrancy guard: this function spins the executor internally,
    // so it must not be called from inside a dispatch callback.
    let exec_ref = &*executor;
    if exec_ref.in_dispatch {
        return RCLC_RET_REENTRANT;
    }

    // Send async
    let ret = rclc_action_send_goal_async(client, goal, goal_len, goal_uuid);
    if ret != RCL_RET_OK {
        return ret;
    }

    // Install a temporary goal_response callback that sets a flag.
    // The arena's action_client_raw_try_process fires the trampoline
    // during spin_once, which reads client.goal_response_callback.
    //
    // Phase 214.G.1 — replaced `static mut BLOCKING_ACCEPTED: i32` with
    // an `AtomicI32` so the callback + spin loop have an explicit
    // happens-before via Release/Acquire ordering. Same single-call
    // contract (this blocking API expects one concurrent invocation)
    // but no longer relies on the absent fence between the callback's
    // store and the loop's load.
    let client_ref = &mut *client;
    use core::sync::atomic::{AtomicI32, Ordering};
    static BLOCKING_ACCEPTED: AtomicI32 = AtomicI32::new(-1); // -1=pending, 0=rejected, 1=accepted
    BLOCKING_ACCEPTED.store(-1, Ordering::Relaxed);

    let orig_cb = client_ref.goal_response_callback;
    let orig_ctx = client_ref.context;
    unsafe extern "C" fn blocking_goal_cb(
        _uuid: *const rclc_goal_uuid_t,
        accepted: bool,
        _ctx: *mut core::ffi::c_void,
    ) {
        BLOCKING_ACCEPTED.store(if accepted { 1 } else { 0 }, Ordering::Release);
    }
    client_ref.goal_response_callback = Some(blocking_goal_cb);

    // Phase 89.12: wall-clock budget — same fix as 89.2 for the service
    // client. The old `for _ in 0..1000` loop assumed each
    // `spin_some(10ms)` actually spent ~10 ms, but on multi-threaded
    // zpico backends the inner condvar can wake on any incoming frame
    // (keep-alives, discovery gossip) and 1000 iterations exhaust in
    // milliseconds. On NuttX QEMU cold-boot the server-side goal
    // response easily slides past that window — budget by the clock.
    const ACTION_BLOCKING_TIMEOUT_MS: u64 = 15_000;
    let start_ns = crate::platform::get_time_ns();
    let timeout_ns: u64 = ACTION_BLOCKING_TIMEOUT_MS.saturating_mul(1_000_000);
    loop {
        crate::executor::rclc_executor_spin_some(executor, 10_000_000);
        let flag = BLOCKING_ACCEPTED.load(Ordering::Acquire);
        if flag >= 0 {
            client_ref.goal_response_callback = orig_cb;
            client_ref.context = orig_ctx;
            // issue 0868 — a server REJECTION is `REJECTED`, not `ERROR`.
            // Same split as the C++ twin `nros_cpp_action_client_send_goal`:
            // `ERROR` is what the async send below returns when the goal
            // could not be sent at all, so reusing it here left the caller
            // unable to tell "the server said no" from "it never left".
            return if flag == 1 {
                RCL_RET_OK
            } else {
                RCLC_RET_REJECTED
            };
        }
        let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
        if elapsed_ns >= timeout_ns {
            break;
        }
    }
    client_ref.goal_response_callback = orig_cb;
    client_ref.context = orig_ctx;
    RCL_RET_TIMEOUT
}

/// Request cancellation of a goal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_cancel_goal(
    client: *mut rcl_action_client_t,
    goal_uuid: *const rclc_goal_uuid_t,
) -> rcl_ret_t {
    validate_not_null!(client, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;
    let uuid = &*goal_uuid;
    let goal_id = nros_node::GoalUuid { uuid: uuid.uuid };

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return RCL_RET_NOT_INIT,
    };
    match core.send_cancel_request(&goal_id) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCL_RET_ERROR,
    }
}

/// Request result of a goal (blocking convenience).
///
/// Calls `rclc_action_get_result_async` then spins the executor until the
/// result arrives or timeout. Never calls `zpico_get` directly.
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub unsafe extern "C" fn rclc_action_get_result(
    client: *mut rcl_action_client_t,
    executor: *mut crate::executor::rclc_executor_t,
    goal_uuid: *const rclc_goal_uuid_t,
    status: *mut rclc_goal_status_t,
    result: *mut u8,
    result_capacity: usize,
    result_len: *mut usize,
) -> rcl_ret_t {
    validate_not_null!(client, executor, goal_uuid, status, result, result_len);

    // Reentrancy guard: this function spins the executor internally,
    // so it must not be called from inside a dispatch callback.
    let exec_ref = &*executor;
    if exec_ref.in_dispatch {
        return RCLC_RET_REENTRANT;
    }

    // Send get_result request async
    let ret = rclc_action_get_result_async(client, goal_uuid);
    if ret != RCL_RET_OK {
        return ret;
    }

    // Install temporary result callback that captures the result into static buffers.
    //
    // Phase 214.G.1 — replaced `static mut BLK_RESULT_{LEN,STATUS}: i32`/`u8`
    // with `AtomicI32`/`AtomicU8`, and wrapped the byte buffer in a
    // `Sync`-asserting `UnsafeCell`. The callback writes the buffer +
    // STATUS then publishes via a `Release` store to LEN; the loop
    // observes LEN with `Acquire` and that fences the buffer read.
    // Single-call contract preserved.
    let client_ref = &mut *client;
    const BLK_RESULT_BUF_LEN: usize = 1024; // max captured action-result CDR
    use core::{
        cell::UnsafeCell,
        sync::atomic::{AtomicI32, AtomicU8, Ordering},
    };
    struct BlkResultBuf(UnsafeCell<[u8; BLK_RESULT_BUF_LEN]>);
    // SAFETY: cross-thread access is gated by BLK_RESULT_LEN's
    // Release/Acquire pair — the callback's store(Release) happens-
    // before the loop's load(Acquire) ≥ 0, which fences the buffer
    // write.
    unsafe impl Sync for BlkResultBuf {}
    static BLK_RESULT_LEN: AtomicI32 = AtomicI32::new(-1);
    static BLK_RESULT_STATUS: AtomicU8 = AtomicU8::new(0);
    static BLK_RESULT_BUF: BlkResultBuf = BlkResultBuf(UnsafeCell::new([0u8; BLK_RESULT_BUF_LEN]));
    BLK_RESULT_LEN.store(-1, Ordering::Relaxed);

    let orig_cb = client_ref.result_callback;
    let orig_ctx = client_ref.context;
    unsafe extern "C" fn blk_result_cb(
        _uuid: *const rclc_goal_uuid_t,
        st: rclc_goal_status_t,
        data: *const u8,
        len: usize,
        _ctx: *mut core::ffi::c_void,
    ) {
        BLK_RESULT_STATUS.store(st as u8, Ordering::Relaxed);
        let copy_len = len.min(1024);
        // SAFETY: BLK_RESULT_BUF lives forever; this fn runs in the
        // executor spin context that called the blocking API and only
        // one such call is in flight by contract.
        unsafe {
            core::ptr::copy_nonoverlapping(data, (*BLK_RESULT_BUF.0.get()).as_mut_ptr(), copy_len);
        }
        // Publish: any loop observing LEN ≥ 0 with Acquire sees the
        // buffer write above.
        BLK_RESULT_LEN.store(copy_len as i32, Ordering::Release);
    }
    client_ref.result_callback = Some(blk_result_cb);

    // Phase 89.12: wall-clock budget (same fix as the send_goal side).
    // Actions can legitimately run for several seconds; 30 s gives
    // room for a Fibonacci-10 feedback stream + result over QEMU slirp.
    const ACTION_RESULT_TIMEOUT_MS: u64 = 30_000;
    let start_ns = crate::platform::get_time_ns();
    let timeout_ns: u64 = ACTION_RESULT_TIMEOUT_MS.saturating_mul(1_000_000);
    loop {
        crate::executor::rclc_executor_spin_some(executor, 10_000_000);
        let rlen = BLK_RESULT_LEN.load(Ordering::Acquire);
        if rlen >= 0 {
            client_ref.result_callback = orig_cb;
            client_ref.context = orig_ctx;
            let data_len = rlen as usize;

            *status = match BLK_RESULT_STATUS.load(Ordering::Relaxed) {
                1 => rclc_goal_status_t::RCLC_GOAL_STATUS_ACCEPTED,
                2 => rclc_goal_status_t::RCLC_GOAL_STATUS_EXECUTING,
                3 => rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELING,
                4 => rclc_goal_status_t::RCLC_GOAL_STATUS_SUCCEEDED,
                5 => rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELED,
                6 => rclc_goal_status_t::RCLC_GOAL_STATUS_ABORTED,
                _ => rclc_goal_status_t::RCLC_GOAL_STATUS_UNKNOWN,
            };

            // Issue #179 — the dispatch paths now deliver the result WITH its
            // CDR encapsulation header (arena + poll both splice one in when a
            // C server's reply arrives raw), so copy verbatim. The old
            // unconditional re-header here double-framed a typed server's
            // reply and, combined with the poll path's off-by-alignment slice,
            // broke every zenoh C/C++ action client.
            if data_len > result_capacity {
                return RCL_RET_ERROR;
            }
            let out = core::slice::from_raw_parts_mut(result, result_capacity);
            // SAFETY: BLK_RESULT_LEN's Acquire load above fences the
            // buffer write in `blk_result_cb`.
            let buf = unsafe { &*BLK_RESULT_BUF.0.get() };
            out[..data_len].copy_from_slice(&buf[..data_len]);
            *result_len = data_len;
            return RCL_RET_OK;
        }
        let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
        if elapsed_ns >= timeout_ns {
            break;
        }
    }
    client_ref.result_callback = orig_cb;
    client_ref.context = orig_ctx;
    RCL_RET_TIMEOUT
}

/// Try to receive feedback for an active goal (non-blocking).
///
/// If feedback is available, invokes the feedback callback (if set).
/// Returns `RCL_RET_OK` if feedback was received and dispatched,
/// `RCL_RET_TIMEOUT` if no feedback is available.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_try_recv_feedback(
    client: *mut rcl_action_client_t,
) -> rcl_ret_t {
    validate_not_null!(client);

    let client = &mut *client;

    validate_state!(
        client,
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return RCL_RET_NOT_INIT,
    };

    match core.try_recv_feedback_raw() {
        Ok(Some((goal_id, len))) => {
            if let Some(cb) = client.feedback_callback {
                let uuid = rclc_goal_uuid_t { uuid: goal_id.uuid };

                // Issue 2014 — offset and splice from `action_frame`: the C
                // deserializer expects [CDR_HEADER][fields], the frame's own
                // header in front, as the executor dispatch delivers.
                const FB_USER_BUF: usize = 512;
                let mut fb_buf = [0u8; FB_USER_BUF];
                if let Some(n) =
                    action_frame::splice_feedback(&core.feedback_buffer_ref()[..len], &mut fb_buf)
                {
                    cb(&uuid, fb_buf.as_ptr(), n, client.context);
                } else {
                    cb(&uuid, ptr::null(), 0, client.context);
                }
            }

            RCL_RET_OK
        }
        Ok(None) => RCL_RET_TIMEOUT,
        Err(_) => RCL_RET_ERROR,
    }
}

// ============================================================================
// Async (non-blocking) action client functions
// ============================================================================

/// Send a goal asynchronously (non-blocking).
///
/// Sends the goal request and returns immediately. The goal response
/// (accepted/rejected) arrives via the goal_response_callback registered
/// with `rclc_action_client_set_goal_response_callback`, invoked during
/// `rclc_executor_spin_some`.
///
/// The `goal_uuid` output is filled with the generated goal UUID on success.
///
/// # Safety
/// All pointers must be valid. `goal` must point to `goal_len` bytes of
/// CDR-serialized goal data (with CDR header).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_send_goal_async(
    client: *mut rcl_action_client_t,
    goal: *const u8,
    goal_len: usize,
    goal_uuid: *mut rclc_goal_uuid_t,
) -> rcl_ret_t {
    validate_not_null!(client, goal, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;
    let goal_data = core::slice::from_raw_parts(goal, goal_len);

    // C serialize produces [CDR_HEADER][fields] — strip the header.
    let goal_fields = strip_cdr_header(goal_data);

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return RCL_RET_NOT_INIT,
    };

    // Non-blocking: uses zpico_get_start internally (not zpico_get).
    match core.send_goal_raw(goal_fields) {
        Ok(goal_id) => {
            let uuid = &mut *goal_uuid;
            uuid.uuid = goal_id.uuid;
            RCL_RET_OK
        }
        Err(_) => RCL_RET_ERROR,
    }
}

/// Request a goal result asynchronously (non-blocking).
///
/// Sends the get_result request and returns immediately. The result
/// arrives via the result_callback registered with
/// `rclc_action_client_set_result_callback`, invoked during
/// `rclc_executor_spin_some`.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_get_result_async(
    client: *mut rcl_action_client_t,
    goal_uuid: *const rclc_goal_uuid_t,
) -> rcl_ret_t {
    validate_not_null!(client, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;
    let uuid = &*goal_uuid;
    let goal_id = nros_node::GoalUuid { uuid: uuid.uuid };

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return RCL_RET_NOT_INIT,
    };

    // Non-blocking: uses zpico_get_start internally.
    match core.send_get_result_request(&goal_id) {
        Ok(()) => RCL_RET_OK,
        Err(_) => RCL_RET_ERROR,
    }
}

/// Set the goal response callback for async goal sending.
///
/// Called during `rclc_executor_spin_some` when the server accepts or
/// rejects a goal sent via `rclc_action_send_goal_async`.
///
/// # Safety
/// `client` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_client_set_goal_response_callback(
    client: *mut rcl_action_client_t,
    callback: rclc_goal_response_callback_t,
    context: *mut c_void,
) -> rcl_ret_t {
    validate_not_null!(client);

    let client = &mut *client;
    client.goal_response_callback = callback;
    if !context.is_null() {
        client.context = context;
    }
    RCL_RET_OK
}

/// Poll the action client for pending async replies (non-blocking).
///
/// **Note**: In the unified design (77.6+), `rclc_executor_spin_some` already
/// dispatches `action_client_raw_try_process` which invokes callbacks. This
/// function is provided for manual polling outside the executor loop.
///
/// # Safety
/// `client` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_action_client_poll(client: *mut rcl_action_client_t) -> rcl_ret_t {
    validate_not_null!(client);

    let client_ref = &mut *client;

    validate_state!(
        client_ref,
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client_ref._internal;
    let ctx = client_ref.context;

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return RCL_RET_NOT_INIT,
    };

    // Poll goal acceptance reply. Issue 2009 — attributed to the goal its
    // request named, not to the most recently sent goal (`goal_counter`).
    if let Ok(Some((total_len, goal_id))) = core.try_recv_send_goal_reply_for()
        && let Some(cb) = client_ref.goal_response_callback
    {
        let accepted = action_frame::goal_accepted(&core.result_buffer_ref()[..total_len]);
        let uuid = rclc_goal_uuid_t { uuid: goal_id.uuid };
        cb(&uuid, accepted, ctx);
    }

    // Poll feedback
    if let Ok(Some((goal_id, total_len))) = core.try_recv_feedback_raw()
        && let Some(cb) = client_ref.feedback_callback
    {
        // Issue 2014 — offset and splice from `action_frame`. This path used
        // to hand the callback the bare fields (no encapsulation header) while
        // the executor dispatch handed it `[encap][fields]`, so one callback
        // saw two framings depending on who took the message off the wire.
        const SPLICE_BUF: usize = 1024;
        let mut spliced = [0u8; SPLICE_BUF];
        if let Some(n) =
            action_frame::splice_feedback(&core.feedback_buffer_ref()[..total_len], &mut spliced)
        {
            let uuid = rclc_goal_uuid_t { uuid: goal_id.uuid };
            cb(&uuid, spliced.as_ptr(), n, ctx);
        }
    }

    // Poll result reply — issue 2009, attributed like the acceptance above.
    // Issue 2014 — status, payload offset (byte 8, after the status and its
    // alignment pad; #179) and splice from `action_frame`. The frame's own
    // encap is spliced in front unconditionally: the encap SNIFF this used
    // was retired from the executor dispatch (a body starting `00 01 00 00`
    // reads as a header).
    if let Ok(Some((total_len, goal_id))) = core.try_recv_get_result_reply_for()
        && let Some(cb) = client_ref.result_callback
    {
        let reply = &core.result_buffer_ref()[..total_len];
        const SPLICE_BUF: usize = 1024;
        let mut spliced = [0u8; SPLICE_BUF];
        if let (Some(status), Some(n)) = (
            action_frame::result_status(reply),
            action_frame::splice_result(reply, &mut spliced),
        ) {
            let c_status = match status {
                nros_node::GoalStatusCode::Succeeded => {
                    rclc_goal_status_t::RCLC_GOAL_STATUS_SUCCEEDED
                }
                nros_node::GoalStatusCode::Canceled => {
                    rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELED
                }
                nros_node::GoalStatusCode::Aborted => rclc_goal_status_t::RCLC_GOAL_STATUS_ABORTED,
                _ => rclc_goal_status_t::RCLC_GOAL_STATUS_UNKNOWN,
            };
            let uuid = rclc_goal_uuid_t { uuid: goal_id.uuid };
            cb(&uuid, c_status, spliced.as_ptr(), n, ctx);
        }
    }

    // Poll cancel reply — issue 2030. Taken whether or not a callback is set:
    // a cancel request holds a request slot until its reply is taken, and this
    // tier has no other reader, so leaving it refused the fifth cancel.
    if let Ok(Some((total_len, goal_id))) = core.try_recv_cancel_reply_for() {
        let reply = &core.result_buffer_ref()[..total_len];
        let code = cancel_return_code_c(action_frame::cancel_return_code(reply));
        let n = action_frame::cancel_goals_count(reply).unwrap_or(0);
        if let Some(cb) = client_ref.cancel_response_callback {
            let uuid = rclc_goal_uuid_t { uuid: goal_id.uuid };
            cb(&uuid, code, n, ctx);
        }
    }

    RCL_RET_OK
}

/// Finalize an action client.
///
/// Issue 1609 — `fini` does NOT remove the client from an executor; its arena
/// entry holds this struct as its callback `context`. Call
/// `rclc_executor_remove_action_client(executor, client)` first, which drops
/// the entry (the action leaves the graph) and brings the executor's
/// `handle_count` back down.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_action_client_fini(
    client: *mut rcl_action_client_t,
    node: *mut crate::node::rcl_node_t,
) -> rcl_ret_t {
    // phase-492 W1.b — rcl's arity: the node the entity was created on.
    // Checked non-NULL only; teardown needs nothing from it (RFC-0089
    // ADOPT-BOUNDED, ledger `c:action_client_fini`).
    validate_not_null!(client, node);

    let client = &mut *client;

    match client.state {
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED => {
            // L2: client lives in executor arena (if registered) —
            // reset metadata only. Releasing the entry needs the owning
            // `rclc_executor_t` (issue 1609): that is
            // `rclc_executor_remove_action_client`.
        }
        _ => return RCL_RET_NOT_INIT,
    }

    client._internal = ActionClientInternal::new();
    client.feedback_callback = None;
    client.result_callback = None;
    client.context = ptr::null_mut();
    client.node = crate::node::rclc_node_ref_t::none();
    client.state = rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_SHUTDOWN;

    RCL_RET_OK
}

// ============================================================================
// Phase 122.3.c.6.b — Layer-1 primitive entry points (caller polls)
// ============================================================================

/// Phase 122.3.c.6.b — initialize an L1 polling-mode action client.
///
/// Creates the 4 transport channels (3 service clients + feedback
/// subscriber) immediately and stores the `ActionClientCore` inline
/// in the `rclc_polling_action_client_t`. The caller drives the goal lifecycle via:
/// * `rclc_polling_action_client_send_goal_raw` — start a new goal; returns
///   the generated 16-byte UUID.
/// * `rclc_polling_action_client_try_recv_goal_response_raw` — poll for the
///   send_goal RPC reply (accepted / rejected).
/// * `rclc_polling_action_client_send_get_result_request_raw` +
///   `_try_recv_result_raw` — fetch the terminal result.
/// * `rclc_polling_action_client_send_cancel_request_raw` +
///   `_try_recv_cancel_response_raw` — cancel a goal.
/// * `rclc_polling_action_client_try_recv_feedback_raw` — drain feedback.
///
/// # Safety
/// All pointers must be valid; `action_name` must be a valid
/// null-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_init(
    client: *mut rclc_polling_action_client_t,
    node: *const rcl_node_t,
    type_info: *const super::common::rclc_action_type_t,
    action_name: *const core::ffi::c_char,
) -> rcl_ret_t {
    validate_not_null!(client, node, type_info, action_name);

    let client_mut = &mut *client;
    let node_ref = &*node;
    let type_info_ref = &*type_info;

    validate_state!(
        client_mut,
        rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_UNINITIALIZED,
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
        let send_goal_client =
            match session.create_client(&send_goal_info, QoSProfile::services_default()) {
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
        let cancel_goal_client =
            match session.create_client(&cancel_goal_info, QoSProfile::services_default()) {
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
        let get_result_client =
            match session.create_client(&get_result_info, QoSProfile::services_default()) {
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
        let feedback_subscriber =
            match session.create_subscription(&feedback_topic, QoSProfile::BEST_EFFORT) {
                Ok(h) => h,
                Err(_) => return RCL_RET_ERROR,
            };

        type Core = nros_node::ActionClientCore<
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
            { crate::config::MESSAGE_BUFFER_SIZE },
        >;
        let core = Core::new(
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
        );
        core::ptr::write(client_mut._opaque.as_mut_ptr() as *mut Core, core);
    }

    client_mut.node = unsafe { crate::node::node_ref_of(node) };
    client_mut.state = rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED;
    RCL_RET_OK
}

/// Finalize a polled action client: drops the inline core, which closes its
/// channels.
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if `client` is NULL
/// * `RCL_RET_NOT_INIT` if it is not initialised
/// * `RCLC_RET_STALE_NODE` if its node has already been finalised (issue 1386)
///
/// # Safety
/// `client` must be a valid pointer.
// The reset path zeroes the probed-size `_opaque` slot, which exceeds
// clippy's 16 KB stack-array threshold. The assignment writes directly
// through the existing `*mut` — no transient stack copy in release.
#[allow(clippy::large_stack_arrays)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_fini(
    client: *mut rclc_polling_action_client_t,
) -> rcl_ret_t {
    validate_not_null!(client);
    let client = &mut *client;
    if client.state != rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED {
        return RCL_RET_NOT_INIT;
    }
    if client.node.is_bound() && !crate::node::node_ref_is_live(client.node) {
        return crate::error::RCLC_RET_STALE_NODE;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        core::ptr::drop_in_place(client._opaque.as_mut_ptr() as *mut PollingClientCore);
        client._opaque = [0u64; crate::opaque_sizes::ACTION_CLIENT_OPAQUE_U64S];
    }
    client.node = crate::node::rclc_node_ref_t::none();
    client.state = rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_SHUTDOWN;
    RCL_RET_OK
}

#[cfg(feature = "rmw-cffi")]
type PollingClientCore = nros_node::ActionClientCore<
    { crate::config::MESSAGE_BUFFER_SIZE },
    { crate::config::MESSAGE_BUFFER_SIZE },
    { crate::config::MESSAGE_BUFFER_SIZE },
>;

#[cfg(feature = "rmw-cffi")]
#[inline]
unsafe fn polling_client_core(
    client: *mut rclc_polling_action_client_t,
) -> Option<&'static mut PollingClientCore> {
    if client.is_null() {
        return None;
    }
    let client_mut = &mut *client;
    if client_mut.state != rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_INITIALIZED {
        return None;
    }
    Some(&mut *(client_mut._opaque.as_mut_ptr() as *mut PollingClientCore))
}

/// Phase 122.3.c.6.b — L1 polling: send a goal. Writes the generated
/// 16-byte UUID into `goal_id_out`. Poll for the accept/reject reply
/// via `rclc_polling_action_client_try_recv_goal_response_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_send_goal_raw(
    client: *mut rclc_polling_action_client_t,
    goal_cdr: *const u8,
    goal_len: usize,
    goal_id_out: *mut [u8; 16],
) -> rcl_ret_t {
    if client.is_null() || goal_id_out.is_null() || (goal_cdr.is_null() && goal_len != 0) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        // Issue 0454 / phase-354 W3 — STRIP the encapsulation header.
        //
        // The parameter is `goal_cdr`: a C caller supplies CDR, which begins
        // with the 4-byte encapsulation header. `PollingActionClientCore::
        // send_goal_raw` wants goal FIELDS — its own doc says "without GoalUuid
        // framing", and it appends the bytes after a header `tx_writer`
        // (`CdrWriter::new_with_header`) has already written. Passing CDR
        // through unstripped therefore put TWO headers on the wire, which is
        // issue 0448's defect reaching a peer.
        //
        // The two INTERNAL callers already did this (`strip_cdr_header(goal_data)`
        // -> `goal_fields`); only the FFI arms did not, so the parameter name
        // stated one contract and the code implemented another.
        let slice = core::slice::from_raw_parts(goal_cdr, goal_len);
        match core.send_goal_raw(strip_cdr_header(slice)) {
            Ok(id) => {
                (*goal_id_out).copy_from_slice(&id.uuid);
                RCL_RET_OK
            }
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (goal_cdr, goal_len, goal_id_out);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: try to receive a send_goal reply
/// (the accept/reject response). Returns `0` when no reply yet, `>0`
/// bytes when one was copied into `buf`, negative on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_try_recv_goal_response_raw(
    client: *mut rclc_polling_action_client_t,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let mut goal_id = [0u8; 16];
    unsafe {
        rclc_polling_action_client_try_recv_goal_response_raw_for(
            client,
            buf,
            buf_len,
            &mut goal_id,
        )
    }
}

/// Issue 2011 — [`rclc_polling_action_client_try_recv_goal_response_raw`] that also writes the goal the reply is ABOUT (the
/// goal of the request whose sequence id it names, issue 2009) into
/// `goal_id_out`. With several goals in flight the bytes alone cannot say
/// which one a reply answers. A reply naming no pending request (an abandoned
/// one's) is dropped. `goal_id_out` is written only when a reply is returned
/// (`> 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_try_recv_goal_response_raw_for(
    client: *mut rclc_polling_action_client_t,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if client.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        match core.try_recv_send_goal_reply_for() {
            Ok(Some((len, id))) => {
                let src = core.result_buffer_ref();
                let copy_len = len.min(buf_len);
                core::ptr::copy_nonoverlapping(src.as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
                copy_len as i32
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (buf, buf_len, goal_id_out);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: send a get_result request for the
/// given goal. Reply lands via `_try_recv_result_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_send_get_result_request_raw(
    client: *mut rclc_polling_action_client_t,
    goal_id: *const [u8; 16],
) -> rcl_ret_t {
    if client.is_null() || goal_id.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let id = nros::GoalUuid { uuid: *goal_id };
        match core.send_get_result_request(&id) {
            Ok(_) => RCL_RET_OK,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = goal_id;
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: try to receive a get_result
/// reply. Returns `0` when no reply yet, `>0` bytes copied, negative
/// on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_try_recv_result_raw(
    client: *mut rclc_polling_action_client_t,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let mut goal_id = [0u8; 16];
    unsafe {
        rclc_polling_action_client_try_recv_result_raw_for(client, buf, buf_len, &mut goal_id)
    }
}

/// Issue 2011 — [`rclc_polling_action_client_try_recv_result_raw`] that also writes the goal the reply is ABOUT (the
/// goal of the request whose sequence id it names, issue 2009) into
/// `goal_id_out`. With several goals in flight the bytes alone cannot say
/// which one a reply answers. A reply naming no pending request (an abandoned
/// one's) is dropped. `goal_id_out` is written only when a reply is returned
/// (`> 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_try_recv_result_raw_for(
    client: *mut rclc_polling_action_client_t,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if client.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        match core.try_recv_get_result_reply_for() {
            Ok(Some((len, id))) => {
                let src = core.result_buffer_ref();
                let copy_len = len.min(buf_len);
                core::ptr::copy_nonoverlapping(src.as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
                copy_len as i32
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (buf, buf_len, goal_id_out);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: send a cancel request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_send_cancel_request_raw(
    client: *mut rclc_polling_action_client_t,
    goal_id: *const [u8; 16],
) -> rcl_ret_t {
    if client.is_null() || goal_id.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let id = nros::GoalUuid { uuid: *goal_id };
        match core.send_cancel_request(&id) {
            Ok(_) => RCL_RET_OK,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = goal_id;
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.c — L1 polling: try to receive the cancel-RPC
/// reply. Returns `0` when no reply yet, `>0` bytes copied into
/// `buf`, negative on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_try_recv_cancel_response_raw(
    client: *mut rclc_polling_action_client_t,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let mut goal_id = [0u8; 16];
    unsafe {
        rclc_polling_action_client_try_recv_cancel_response_raw_for(
            client,
            buf,
            buf_len,
            &mut goal_id,
        )
    }
}

/// Issue 2011 — [`rclc_polling_action_client_try_recv_cancel_response_raw`] that also writes the goal the reply is ABOUT (the
/// goal of the request whose sequence id it names, issue 2009) into
/// `goal_id_out`. With several goals in flight the bytes alone cannot say
/// which one a reply answers. A reply naming no pending request (an abandoned
/// one's) is dropped. `goal_id_out` is written only when a reply is returned
/// (`> 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_try_recv_cancel_response_raw_for(
    client: *mut rclc_polling_action_client_t,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if client.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        match core.try_recv_cancel_reply_for() {
            Ok(Some((len, id))) => {
                let src = core.result_buffer_ref();
                let copy_len = len.min(buf_len);
                core::ptr::copy_nonoverlapping(src.as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
                copy_len as i32
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (buf, buf_len, goal_id_out);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.e — register a C wake callback on the
/// send_goal-reply channel of an L1 polling-mode action client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_set_goal_response_wake_callback(
    client: *mut rclc_polling_action_client_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    set_action_client_wake_callback(client, state, cb, ctx, ClientChannel::GoalResponse)
}

/// Phase 122.3.c.6.e — wake on the cancel-reply channel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_set_cancel_response_wake_callback(
    client: *mut rclc_polling_action_client_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    set_action_client_wake_callback(client, state, cb, ctx, ClientChannel::CancelResponse)
}

/// Phase 122.3.c.6.e — wake on the get_result-reply channel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_set_result_wake_callback(
    client: *mut rclc_polling_action_client_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    set_action_client_wake_callback(client, state, cb, ctx, ClientChannel::Result)
}

/// Phase 122.3.c.6.e — wake on the feedback channel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_set_feedback_wake_callback(
    client: *mut rclc_polling_action_client_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> rcl_ret_t {
    set_action_client_wake_callback(client, state, cb, ctx, ClientChannel::Feedback)
}

enum ClientChannel {
    GoalResponse,
    CancelResponse,
    Result,
    Feedback,
}

unsafe fn set_action_client_wake_callback(
    client: *mut rclc_polling_action_client_t,
    state: *mut crate::service::rclc_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
    channel: ClientChannel,
) -> rcl_ret_t {
    if client.is_null() || state.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        let state_ptr = state as *mut nros_node::c_waker::CWakeState;
        core::ptr::write(
            state_ptr,
            nros_node::c_waker::CWakeState { fn_ptr: cb, ctx },
        );
        let waker = nros_node::c_waker::make_waker(state_ptr);
        let armed = match channel {
            ClientChannel::GoalResponse => core.register_goal_response_waker(&waker),
            ClientChannel::CancelResponse => core.register_cancel_response_waker(&waker),
            ClientChannel::Result => core.register_result_waker(&waker),
            ClientChannel::Feedback => core.register_feedback_waker(&waker),
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
        let _ = (state, cb, ctx, channel);
        RCL_RET_NOT_INIT
    }
}

/// Phase 122.3.c.6.b — L1 polling: try to receive feedback for any
/// goal. Returns `0` when no feedback yet, `>0` bytes copied (with
/// `goal_id_out` filled), negative on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_polling_action_client_try_recv_feedback_raw(
    client: *mut rclc_polling_action_client_t,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if client.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    #[cfg(feature = "rmw-cffi")]
    {
        let core = match polling_client_core(client) {
            Some(c) => c,
            None => return RCL_RET_INVALID_ARGUMENT,
        };
        match core.try_recv_feedback_raw() {
            Ok(Some((id, len))) => {
                let src = core.feedback_buffer_ref();
                let copy_len = len.min(buf_len);
                core::ptr::copy_nonoverlapping(src.as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
                copy_len as i32
            }
            Ok(None) => 0,
            Err(_) => RCL_RET_ERROR,
        }
    }
    #[cfg(not(feature = "rmw-cffi"))]
    {
        let _ = (buf, buf_len, goal_id_out);
        RCL_RET_NOT_INIT
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
        }
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_init_null_ptrs() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::rcl_get_zero_initialized_node();

        // NULL client
        assert_eq!(
            unsafe {
                rcl_action_client_init(
                    ptr::null_mut(),
                    &node,
                    &type_info,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    ptr::null(),
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL node
        let mut cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            unsafe {
                rcl_action_client_init(
                    &mut cli,
                    ptr::null(),
                    &type_info,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    ptr::null(),
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL action_name
        let mut cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            unsafe {
                rcl_action_client_init(&mut cli, &node, &type_info, ptr::null(), ptr::null())
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL type_info
        let mut cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            unsafe {
                rcl_action_client_init(
                    &mut cli,
                    &node,
                    ptr::null(),
                    action_name.as_ptr() as *const core::ffi::c_char,
                    ptr::null(),
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_init_uninit_node() {
        let action_name = b"/fibonacci\0";
        let type_info = dummy_action_type();
        let node = crate::node::rcl_get_zero_initialized_node();

        let mut cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            unsafe {
                rcl_action_client_init(
                    &mut cli,
                    &node,
                    &type_info,
                    action_name.as_ptr() as *const core::ffi::c_char,
                    ptr::null(),
                )
            },
            RCL_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_zero_initialized_state() {
        let cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            cli.state,
            rclc_action_client_state_t::RCLC_ACTION_CLIENT_STATE_UNINITIALIZED,
        );
        assert!(cli.node.is_null());
        assert_eq!(cli._internal.arena_entry_index, -1);
        assert!(cli._internal.executor_ptr.is_null());
        assert!(cli.feedback_callback.is_none());
        assert!(cli.result_callback.is_none());
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_client_fini_null_safety() {
        // NULL → INVALID_ARGUMENT
        assert_eq!(
            unsafe {
                rcl_action_client_fini(
                    ptr::null_mut(),
                    &mut crate::node::rcl_get_zero_initialized_node(),
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // UNINITIALIZED → NOT_INIT
        let mut cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            unsafe {
                rcl_action_client_fini(&mut cli, &mut crate::node::rcl_get_zero_initialized_node())
            },
            RCL_RET_NOT_INIT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_send_goal_null_ptrs() {
        let goal_data = [0u8; 8];
        let mut uuid = rclc_goal_uuid_t::default();

        // NULL client
        assert_eq!(
            unsafe { rclc_action_send_goal(ptr::null_mut(), goal_data.as_ptr(), 8, &mut uuid) },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL goal
        let mut cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            unsafe { rclc_action_send_goal(&mut cli, ptr::null(), 0, &mut uuid) },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL uuid
        assert_eq!(
            unsafe { rclc_action_send_goal(&mut cli, goal_data.as_ptr(), 8, ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn action_get_result_null_ptrs() {
        let uuid = rclc_goal_uuid_t::default();
        let mut status = rclc_goal_status_t::RCLC_GOAL_STATUS_UNKNOWN;
        let mut result_buf = [0u8; 8];
        let mut result_len: usize = 0;

        // NULL client
        assert_eq!(
            unsafe {
                rclc_action_get_result(
                    ptr::null_mut(),
                    &uuid,
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL uuid
        let mut cli = rcl_action_get_zero_initialized_client();
        assert_eq!(
            unsafe {
                rclc_action_get_result(
                    &mut cli,
                    ptr::null(),
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL status
        assert_eq!(
            unsafe {
                rclc_action_get_result(
                    &mut cli,
                    &uuid,
                    ptr::null_mut(),
                    result_buf.as_mut_ptr(),
                    8,
                    &mut result_len,
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL result
        assert_eq!(
            unsafe {
                rclc_action_get_result(
                    &mut cli,
                    &uuid,
                    &mut status,
                    ptr::null_mut(),
                    8,
                    &mut result_len,
                )
            },
            RCL_RET_INVALID_ARGUMENT,
        );

        // NULL result_len
        assert_eq!(
            unsafe {
                rclc_action_get_result(
                    &mut cli,
                    &uuid,
                    &mut status,
                    result_buf.as_mut_ptr(),
                    8,
                    ptr::null_mut(),
                )
            },
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

    /// The client's name accessor answers NULL, not an empty string, for a
    /// handle that was never initialised — phase-417 W4.b.
    ///
    /// The server twin is in `action/server.rs`; the reason is the same and it
    /// is the one a zeroed `action_name` buffer gets wrong silently.
    #[test]
    fn get_action_name_refuses_an_unusable_client() {
        let client = rcl_action_get_zero_initialized_client();
        assert!(unsafe { rcl_action_client_get_action_name(core::ptr::null()) }.is_null());
        assert!(unsafe { rcl_action_client_get_action_name(&client) }.is_null());
    }
}
