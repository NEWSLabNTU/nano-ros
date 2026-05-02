//! Action client implementation.

use core::{ffi::c_void, ptr};

use nros::{
    GoalId,
    cdr::{CDR_HEADER_LEN, strip_cdr_header, write_cdr_le_header},
};

use super::common::*;
use crate::{
    constants::{MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN},
    error::*,
    node::{nros_node_state_t, nros_node_t},
};

/// CDR sequence<uint8, 16> length prefix (4 bytes) in front of the UUID bytes.
/// Mirrors the constant of the same name in `server.rs`.
const GOAL_ID_SEQ_PREFIX_LEN: usize = 4;

/// Bytes of CDR framing that precede feedback fields in a feedback message:
/// CDR encapsulation header + GoalId sequence length prefix + UUID.
const FEEDBACK_FRAMING_LEN: usize = CDR_HEADER_LEN + GOAL_ID_SEQ_PREFIX_LEN + GoalId::UUID_LEN;

// ============================================================================
// Internal implementation
// ============================================================================

/// Internal state for the action client.
///
/// Lightweight — stores only the arena entry index and executor pointer.
/// The `ActionClientCore` (transport handles) lives in the executor's arena,
/// created by `nros_executor_add_action_client`.
#[repr(C)]
pub struct ActionClientInternal {
    /// Arena entry index (set by nros_executor_add_action_client).
    /// -1 means not registered with executor.
    pub arena_entry_index: i32,
    /// Pointer to the executor (set by nros_executor_add_action_client).
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
    /// Goal response callback (for async send_goal)
    pub goal_response_callback: nros_goal_response_callback_t,
    /// Feedback callback
    pub feedback_callback: nros_feedback_callback_t,
    /// Result callback
    pub result_callback: nros_result_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent node
    pub node: *const nros_node_t,
    /// Internal state (arena entry index + executor pointer). Phase 87.5:
    /// Typed C-ABI handle field.
    pub _internal: ActionClientInternal,
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
            goal_response_callback: None,
            feedback_callback: None,
            result_callback: None,
            context: ptr::null_mut(),
            node: ptr::null(),
            _internal: ActionClientInternal::new(),
        }
    }
}

// ============================================================================
// Action Client Functions
// ============================================================================

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
    action_name: *const core::ffi::c_char,
    type_info: *const nros_action_type_t,
) -> nros_ret_t {
    validate_not_null!(client, node, action_name, type_info);

    let client = &mut *client;
    let node_ref = &*node;
    let type_info = &*type_info;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(node_ref, nros_node_state_t::NROS_NODE_STATE_INITIALIZED);

    // Copy action name (required — empty rejected)
    client.action_name_len = crate::util::copy_cstr_into(action_name, &mut client.action_name);
    if client.action_name_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Copy type name + hash (both optional — null sources leave dst untouched)
    client.type_name_len = crate::util::copy_cstr_into(type_info.type_name, &mut client.type_name);
    client.type_hash_len = crate::util::copy_cstr_into(type_info.type_hash, &mut client.type_hash);

    // Store node pointer
    client.node = node;

    // Metadata only — no transport handles created here.
    // Transport handles are created in nros_executor_add_action_client,
    // which places the ActionClientCore in the executor's arena.
    client._internal = ActionClientInternal::new();

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
    validate_not_null!(client);

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
    validate_not_null!(client);

    let client = &mut *client;
    client.result_callback = callback;
    client.context = context;

    NROS_RET_OK
}

/// Block until the action server's send-goal queryable is discoverable
/// on the network, or `timeout_ms` elapses.
///
/// Mirrors `rclcpp_action::Client::wait_for_action_server` and the
/// the underlying `ActionClient::wait_for_action_server`. Internally
/// probes the action's `send_goal` service-server liveliness keyexpr
/// (the goal queryable is the load-bearing entity for the first
/// `nros_action_send_goal` call) via the same primitive as the
/// service-client equivalent. See
/// `packages/core/nros-c/src/service.rs::nros_client_wait_for_service`
/// for the re-probe rationale.
///
/// # Returns
/// * `NROS_RET_OK` — server visible.
/// * `NROS_RET_TIMEOUT` — `timeout_ms` elapsed without seeing a token.
/// * `NROS_RET_NOT_INIT` — client not registered with an executor.
/// * `NROS_RET_ERROR` — transport-level failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_wait_for_action_server(
    client: *mut nros_action_client_t,
    executor: *mut crate::executor::nros_executor_t,
    timeout_ms: u32,
) -> nros_ret_t {
    validate_not_null!(client, executor);

    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
    {
        let client_ref = &mut *client;
        if client_ref.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED {
            return NROS_RET_NOT_INIT;
        }
        let internal = &mut client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return NROS_RET_NOT_INIT;
        }
        // Note: action clients store an opaque pointer into
        // `executor._opaque` (see `nros_executor_add_action_client`),
        // not to the outer `nros_executor_t`, so we can't recover the
        // wrapper from `internal.executor_ptr`. Take `executor` as a
        // separate argument — same convention as
        // `nros_action_send_goal`.
        let exec_t = &mut *executor;
        if exec_t.in_dispatch {
            return NROS_RET_REENTRANT;
        }

        // Latched fast-path.
        {
            let exec = crate::executor::get_executor(&mut exec_t._opaque);
            let core = match exec.action_client_core_mut(internal.arena_entry_index as usize) {
                Some(c) => c,
                None => return NROS_RET_NOT_INIT,
            };
            if core.is_server_ready() {
                return NROS_RET_OK;
            }
        }

        const PROBE_TIMEOUT_MS: u32 = 1000;
        let start_ns = crate::platform::get_time_ns();
        let timeout_ns: u64 = (timeout_ms as u64).saturating_mul(1_000_000);
        loop {
            {
                let exec = crate::executor::get_executor(&mut exec_t._opaque);
                let core = match exec.action_client_core_mut(internal.arena_entry_index as usize) {
                    Some(c) => c,
                    None => return NROS_RET_NOT_INIT,
                };
                if core.start_server_discovery(PROBE_TIMEOUT_MS).is_err() {
                    return NROS_RET_ERROR;
                }
            }

            loop {
                crate::executor::nros_executor_spin_some(executor, 10_000_000);

                let exec = crate::executor::get_executor(&mut exec_t._opaque);
                let core = match exec.action_client_core_mut(internal.arena_entry_index as usize) {
                    Some(c) => c,
                    None => return NROS_RET_NOT_INIT,
                };
                match core.poll_server_discovery() {
                    Ok(Some(true)) => return NROS_RET_OK,
                    Ok(Some(false)) => break,
                    Ok(None) => {}
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

    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")))]
    {
        let _ = (client, executor, timeout_ms);
        NROS_RET_OK
    }
}

/// Non-blocking snapshot of action-server visibility. Mirrors
/// `rclcpp_action::Client::action_server_is_ready`. Takes `executor`
/// for the same reason as
/// [`nros_action_client_wait_for_action_server`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_action_server_is_ready(
    client: *const nros_action_client_t,
    executor: *mut crate::executor::nros_executor_t,
) -> bool {
    if client.is_null() || executor.is_null() {
        return false;
    }
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
    {
        let client_ref = &*client;
        if client_ref.state != nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED {
            return false;
        }
        let internal = &client_ref._internal;
        if internal.executor_ptr.is_null() || internal.arena_entry_index < 0 {
            return false;
        }
        let exec_t = &mut *executor;
        let exec = crate::executor::get_executor(&mut exec_t._opaque);
        match exec.action_client_core_mut(internal.arena_entry_index as usize) {
            Some(core) => core.is_server_ready(),
            None => false,
        }
    }
    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")))]
    {
        let _ = (client, executor);
        true
    }
}

/// Send a goal request.
#[unsafe(no_mangle)]
/// Send a goal (blocking convenience).
///
/// Calls `nros_action_send_goal_async` then spins the executor until the
/// goal is accepted/rejected or timeout. Never calls `zpico_get` directly —
/// all I/O is driven by the executor's `spin_once`.
///
/// Like the runtime's `Promise::wait`, this is syntactic sugar over async + spin.
#[allow(static_mut_refs)]
pub unsafe extern "C" fn nros_action_send_goal(
    client: *mut nros_action_client_t,
    executor: *mut crate::executor::nros_executor_t,
    goal: *const u8,
    goal_len: usize,
    goal_uuid: *mut nros_goal_uuid_t,
) -> nros_ret_t {
    validate_not_null!(client, goal, goal_uuid, executor);

    // Reentrancy guard: this function spins the executor internally,
    // so it must not be called from inside a dispatch callback.
    let exec_ref = &*executor;
    if exec_ref.in_dispatch {
        return NROS_RET_REENTRANT;
    }

    // Send async
    let ret = nros_action_send_goal_async(client, goal, goal_len, goal_uuid);
    if ret != NROS_RET_OK {
        return ret;
    }

    // Install a temporary goal_response callback that sets a flag.
    // The arena's action_client_raw_try_process fires the trampoline
    // during spin_once, which reads client.goal_response_callback.
    let client_ref = &mut *client;
    static mut BLOCKING_ACCEPTED: i32 = -1; // -1=pending, 0=rejected, 1=accepted
    BLOCKING_ACCEPTED = -1;

    let orig_cb = client_ref.goal_response_callback;
    let orig_ctx = client_ref.context;
    unsafe extern "C" fn blocking_goal_cb(
        _uuid: *const nros_goal_uuid_t,
        accepted: bool,
        _ctx: *mut core::ffi::c_void,
    ) {
        unsafe {
            BLOCKING_ACCEPTED = if accepted { 1 } else { 0 };
        }
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
        crate::executor::nros_executor_spin_some(executor, 10_000_000);
        let flag = BLOCKING_ACCEPTED;
        if flag >= 0 {
            client_ref.goal_response_callback = orig_cb;
            client_ref.context = orig_ctx;
            return if flag == 1 {
                NROS_RET_OK
            } else {
                NROS_RET_ERROR
            };
        }
        let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
        if elapsed_ns >= timeout_ns {
            break;
        }
    }
    client_ref.goal_response_callback = orig_cb;
    client_ref.context = orig_ctx;
    NROS_RET_TIMEOUT
}

/// Request cancellation of a goal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_cancel_goal(
    client: *mut nros_action_client_t,
    goal_uuid: *const nros_goal_uuid_t,
) -> nros_ret_t {
    validate_not_null!(client, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;
    let uuid = &*goal_uuid;
    let goal_id = nros_node::GoalId { uuid: uuid.uuid };

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return NROS_RET_NOT_INIT,
    };
    match core.send_cancel_request(&goal_id) {
        Ok(()) => NROS_RET_OK,
        Err(_) => NROS_RET_ERROR,
    }
}

/// Request result of a goal (blocking convenience).
///
/// Calls `nros_action_get_result_async` then spins the executor until the
/// result arrives or timeout. Never calls `zpico_get` directly.
#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub unsafe extern "C" fn nros_action_get_result(
    client: *mut nros_action_client_t,
    executor: *mut crate::executor::nros_executor_t,
    goal_uuid: *const nros_goal_uuid_t,
    status: *mut nros_goal_status_t,
    result: *mut u8,
    result_capacity: usize,
    result_len: *mut usize,
) -> nros_ret_t {
    validate_not_null!(client, executor, goal_uuid, status, result, result_len);

    // Reentrancy guard: this function spins the executor internally,
    // so it must not be called from inside a dispatch callback.
    let exec_ref = &*executor;
    if exec_ref.in_dispatch {
        return NROS_RET_REENTRANT;
    }

    // Send get_result request async
    let ret = nros_action_get_result_async(client, goal_uuid);
    if ret != NROS_RET_OK {
        return ret;
    }

    // Install temporary result callback that captures the result into static buffers.
    let client_ref = &mut *client;
    static mut BLK_RESULT_LEN: i32 = -1;
    static mut BLK_RESULT_STATUS: u8 = 0;
    static mut BLK_RESULT_BUF: [u8; 1024] = [0u8; 1024];
    BLK_RESULT_LEN = -1;

    let orig_cb = client_ref.result_callback;
    let orig_ctx = client_ref.context;
    unsafe extern "C" fn blk_result_cb(
        _uuid: *const nros_goal_uuid_t,
        st: nros_goal_status_t,
        data: *const u8,
        len: usize,
        _ctx: *mut core::ffi::c_void,
    ) {
        unsafe {
            BLK_RESULT_STATUS = st as u8;
            let copy_len = len.min(1024);
            core::ptr::copy_nonoverlapping(data, BLK_RESULT_BUF.as_mut_ptr(), copy_len);
            BLK_RESULT_LEN = copy_len as i32;
        }
    }
    client_ref.result_callback = Some(blk_result_cb);

    // Phase 89.12: wall-clock budget (same fix as the send_goal side).
    // Actions can legitimately run for several seconds; 30 s gives
    // room for a Fibonacci-10 feedback stream + result over QEMU slirp.
    const ACTION_RESULT_TIMEOUT_MS: u64 = 30_000;
    let start_ns = crate::platform::get_time_ns();
    let timeout_ns: u64 = ACTION_RESULT_TIMEOUT_MS.saturating_mul(1_000_000);
    loop {
        crate::executor::nros_executor_spin_some(executor, 10_000_000);
        let rlen = BLK_RESULT_LEN;
        if rlen >= 0 {
            client_ref.result_callback = orig_cb;
            client_ref.context = orig_ctx;
            let data_len = rlen as usize;

            *status = match BLK_RESULT_STATUS {
                1 => nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED,
                2 => nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING,
                3 => nros_goal_status_t::NROS_GOAL_STATUS_CANCELING,
                4 => nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
                5 => nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
                6 => nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
                _ => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
            };

            // Prepend CDR header for C deserializer
            let output_len = CDR_HEADER_LEN + data_len;
            if output_len > result_capacity {
                return NROS_RET_ERROR;
            }
            let out = core::slice::from_raw_parts_mut(result, result_capacity);
            let payload = write_cdr_le_header(out).expect("out_capacity >= CDR_HEADER_LEN");
            payload[..data_len].copy_from_slice(&BLK_RESULT_BUF[..data_len]);
            *result_len = output_len;
            return NROS_RET_OK;
        }
        let elapsed_ns = crate::platform::get_time_ns().saturating_sub(start_ns);
        if elapsed_ns >= timeout_ns {
            break;
        }
    }
    client_ref.result_callback = orig_cb;
    client_ref.context = orig_ctx;
    NROS_RET_TIMEOUT
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
    validate_not_null!(client);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return NROS_RET_NOT_INIT,
    };

    match core.try_recv_feedback_raw() {
        Ok(Some((goal_id, len))) => {
            if let Some(cb) = client.feedback_callback {
                let uuid = nros_goal_uuid_t { uuid: goal_id.uuid };

                // Feedback wire layout: [CDR_HEADER][GoalId seq prefix][UUID][feedback fields].
                // The C deserializer expects [CDR_HEADER][fields], so we
                // prepend a CDR header in a stack buffer.
                let fb_fields_len = len.saturating_sub(FEEDBACK_FRAMING_LEN);

                if fb_fields_len > 0 {
                    let mut fb_buf = [0u8; 512];
                    let payload =
                        write_cdr_le_header(&mut fb_buf).expect("fb_buf >= CDR_HEADER_LEN");
                    let copy_len = fb_fields_len.min(payload.len());
                    payload[..copy_len].copy_from_slice(
                        &core.feedback_buffer_ref()
                            [FEEDBACK_FRAMING_LEN..FEEDBACK_FRAMING_LEN + copy_len],
                    );
                    cb(
                        &uuid,
                        fb_buf.as_ptr(),
                        CDR_HEADER_LEN + copy_len,
                        client.context,
                    );
                } else {
                    cb(&uuid, ptr::null(), 0, client.context);
                }
            }

            NROS_RET_OK
        }
        Ok(None) => NROS_RET_TIMEOUT,
        Err(_) => NROS_RET_ERROR,
    }
}

// ============================================================================
// Async (non-blocking) action client functions
// ============================================================================

/// Send a goal asynchronously (non-blocking).
///
/// Sends the goal request and returns immediately. The goal response
/// (accepted/rejected) arrives via the goal_response_callback registered
/// with `nros_action_client_set_goal_response_callback`, invoked during
/// `nros_executor_spin_some`.
///
/// The `goal_uuid` output is filled with the generated goal UUID on success.
///
/// # Safety
/// All pointers must be valid. `goal` must point to `goal_len` bytes of
/// CDR-serialized goal data (with CDR header).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_send_goal_async(
    client: *mut nros_action_client_t,
    goal: *const u8,
    goal_len: usize,
    goal_uuid: *mut nros_goal_uuid_t,
) -> nros_ret_t {
    validate_not_null!(client, goal, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;
    let goal_data = core::slice::from_raw_parts(goal, goal_len);

    // C serialize produces [CDR_HEADER][fields] — strip the header.
    let goal_fields = strip_cdr_header(goal_data);

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return NROS_RET_NOT_INIT,
    };

    // Non-blocking: uses zpico_get_start internally (not zpico_get).
    match core.send_goal_raw(goal_fields) {
        Ok(goal_id) => {
            let uuid = &mut *goal_uuid;
            uuid.uuid = goal_id.uuid;
            NROS_RET_OK
        }
        Err(_) => NROS_RET_ERROR,
    }
}

/// Request a goal result asynchronously (non-blocking).
///
/// Sends the get_result request and returns immediately. The result
/// arrives via the result_callback registered with
/// `nros_action_client_set_result_callback`, invoked during
/// `nros_executor_spin_some`.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_get_result_async(
    client: *mut nros_action_client_t,
    goal_uuid: *const nros_goal_uuid_t,
) -> nros_ret_t {
    validate_not_null!(client, goal_uuid);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client._internal;
    let uuid = &*goal_uuid;
    let goal_id = nros_node::GoalId { uuid: uuid.uuid };

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return NROS_RET_NOT_INIT,
    };

    // Non-blocking: uses zpico_get_start internally.
    match core.send_get_result_request(&goal_id) {
        Ok(()) => NROS_RET_OK,
        Err(_) => NROS_RET_ERROR,
    }
}

/// Set the goal response callback for async goal sending.
///
/// Called during `nros_executor_spin_some` when the server accepts or
/// rejects a goal sent via `nros_action_send_goal_async`.
///
/// # Safety
/// `client` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_set_goal_response_callback(
    client: *mut nros_action_client_t,
    callback: nros_goal_response_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    validate_not_null!(client);

    let client = &mut *client;
    client.goal_response_callback = callback;
    if !context.is_null() {
        client.context = context;
    }
    NROS_RET_OK
}

/// Poll the action client for pending async replies (non-blocking).
///
/// **Note**: In the unified design (77.6+), `nros_executor_spin_some` already
/// dispatches `action_client_raw_try_process` which invokes callbacks. This
/// function is provided for manual polling outside the executor loop.
///
/// # Safety
/// `client` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_poll(client: *mut nros_action_client_t) -> nros_ret_t {
    validate_not_null!(client);

    let client_ref = &mut *client;

    validate_state!(
        client_ref,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    let internal = &mut client_ref._internal;
    let ctx = client_ref.context;

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return NROS_RET_NOT_INIT,
    };

    // Poll goal acceptance reply
    if let Ok(Some(total_len)) = core.try_recv_send_goal_reply()
        && let Some(cb) = client_ref.goal_response_callback
    {
        let buf = core.result_buffer_ref();
        let accepted = total_len >= 5 && buf[4] != 0;
        let uuid = nros_goal_uuid_t {
            uuid: {
                let mut u = [0u8; 16];
                u[..8].copy_from_slice(&core.goal_counter().to_le_bytes());
                u
            },
        };
        cb(&uuid, accepted, ctx);
    }

    // Poll feedback
    if let Ok(Some((goal_id, total_len))) = core.try_recv_feedback_raw()
        && let Some(cb) = client_ref.feedback_callback
    {
        let buf = core.feedback_buffer_ref();
        // NOTE: this polling path uses CDR_HEADER + UUID (20 bytes) whereas
        // `nros_action_try_recv_feedback` above uses CDR_HEADER + seq_prefix
        // + UUID (24). The discrepancy is tracked as a Phase 83 audit finding
        // — preserve current semantics until the feedback framing is unified
        // inside `ActionClientCore`.
        let offset = CDR_HEADER_LEN + GoalId::UUID_LEN;
        if total_len > offset {
            let uuid = nros_goal_uuid_t { uuid: goal_id.uuid };
            cb(
                &uuid,
                buf[offset..total_len].as_ptr(),
                total_len - offset,
                ctx,
            );
        }
    }

    // Poll result reply
    if let Ok(Some(total_len)) = core.try_recv_get_result_reply()
        && let Some(cb) = client_ref.result_callback
    {
        let buf = core.result_buffer_ref();
        if total_len >= 5 {
            let status_byte = buf[4];
            let c_status = match status_byte {
                4 => nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
                5 => nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
                6 => nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
                _ => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
            };
            let result_offset = 5;
            let uuid = nros_goal_uuid_t {
                uuid: {
                    let mut u = [0u8; 16];
                    u[..8].copy_from_slice(&core.goal_counter().to_le_bytes());
                    u
                },
            };
            cb(
                &uuid,
                c_status,
                buf[result_offset..total_len].as_ptr(),
                total_len - result_offset,
                ctx,
            );
        }
    }

    NROS_RET_OK
}

/// Finalize an action client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_action_client_fini(client: *mut nros_action_client_t) -> nros_ret_t {
    validate_not_null!(client);

    let client = &mut *client;

    validate_state!(
        client,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    // Reset the inline ActionClientInternal. The ActionClientCore
    // (transport handles) lives in the executor's arena and is freed when
    // the executor is destroyed.
    client._internal = ActionClientInternal::new();
    client.feedback_callback = None;
    client.result_callback = None;
    client.context = ptr::null_mut();
    client.node = ptr::null();
    client.state = nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_SHUTDOWN;

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
}
