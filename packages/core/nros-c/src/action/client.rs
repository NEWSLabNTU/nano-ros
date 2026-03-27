//! Action client implementation.

use core::ffi::c_void;
use core::ptr;

use super::common::*;
use crate::config::ACTION_CLIENT_INTERNAL_OPAQUE_U64S;
use crate::constants::{MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};
use crate::error::*;
use crate::node::{nros_node_state_t, nros_node_t};

// ============================================================================
// Internal implementation
// ============================================================================

/// Internal state for the action client.
///
/// Lightweight — stores only the arena entry index and executor pointer.
/// The `ActionClientCore` (transport handles) lives in the executor's arena,
/// created by `nros_executor_add_action_client`.
pub(crate) struct ActionClientInternal {
    /// Arena entry index (set by nros_executor_add_action_client).
    /// -1 means not registered with executor.
    pub(crate) arena_entry_index: i32,
    /// Pointer to the Rust executor (set by nros_executor_add_action_client).
    pub(crate) executor_ptr: *mut core::ffi::c_void,
}

impl ActionClientInternal {
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
    /// Opaque inline storage for internal implementation
    pub _internal: [u64; ACTION_CLIENT_INTERNAL_OPAQUE_U64S],
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
            _internal: [0u64; ACTION_CLIENT_INTERNAL_OPAQUE_U64S],
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

    // Metadata only — no transport handles created here.
    // Transport handles are created in nros_executor_add_action_client,
    // which places the ActionClientCore in the executor's arena.
    let internal = ActionClientInternal {
        arena_entry_index: -1,
        executor_ptr: core::ptr::null_mut(),
    };
    core::ptr::write(
        client._internal.as_mut_ptr() as *mut ActionClientInternal,
        internal,
    );

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

/// Send a goal request.
#[unsafe(no_mangle)]
/// Send a goal (blocking convenience).
///
/// Calls `nros_action_send_goal_async` then spins the executor until the
/// goal is accepted/rejected or timeout. Never calls `zpico_get` directly —
/// all I/O is driven by the executor's `spin_once`.
///
/// Like Rust's `Promise::wait`, this is syntactic sugar over async + spin.
#[allow(static_mut_refs)]
pub unsafe extern "C" fn nros_action_send_goal(
    client: *mut nros_action_client_t,
    executor: *mut crate::executor::nros_executor_t,
    goal: *const u8,
    goal_len: usize,
    goal_uuid: *mut nros_goal_uuid_t,
) -> nros_ret_t {
    validate_not_null!(client, goal, goal_uuid, executor);

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

    // Spin executor until flag set or timeout (~10s = 1000 × 10ms)
    for _ in 0..1000 {
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

    let internal = &mut *(client._internal.as_mut_ptr() as *mut ActionClientInternal);
    let uuid = &*goal_uuid;
    let goal_id = nros_core::GoalId { uuid: uuid.uuid };

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

    // Spin executor until flag set or timeout (~10s = 1000 × 10ms)
    for _ in 0..1000 {
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
            let output_len = 4 + data_len;
            if output_len > result_capacity {
                return NROS_RET_ERROR;
            }
            let out = core::slice::from_raw_parts_mut(result, result_capacity);
            out[0] = 0x00;
            out[1] = 0x01;
            out[2] = 0x00;
            out[3] = 0x00;
            out[4..4 + data_len].copy_from_slice(&BLK_RESULT_BUF[..data_len]);
            *result_len = output_len;
            return NROS_RET_OK;
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

    let internal = &mut *(client._internal.as_mut_ptr() as *mut ActionClientInternal);

    let core = match unsafe { internal.arena_core_mut() } {
        Some(c) => c,
        None => return NROS_RET_NOT_INIT,
    };

    match core.try_recv_feedback_raw() {
        Ok(Some((goal_id, len))) => {
            if let Some(cb) = client.feedback_callback {
                let uuid = nros_goal_uuid_t { uuid: goal_id.uuid };

                // Feedback CDR layout: header (4) + GoalId seq_len (4) + UUID (16) = 24 bytes
                // After offset 24: raw feedback fields (no CDR header, since
                // the server strips it in nros_action_publish_feedback).
                // The C deserializer expects [CDR_HEADER][fields], so we
                // prepend a CDR header in a stack buffer.
                let fb_offset = 24usize;
                let fb_fields_len = len.saturating_sub(fb_offset);

                if fb_fields_len > 0 {
                    let mut fb_buf = [0u8; 512];
                    fb_buf[0] = 0x00;
                    fb_buf[1] = 0x01;
                    fb_buf[2] = 0x00;
                    fb_buf[3] = 0x00;
                    let copy_len = fb_fields_len.min(fb_buf.len() - 4);
                    fb_buf[4..4 + copy_len].copy_from_slice(
                        &core.feedback_buffer_ref()[fb_offset..fb_offset + copy_len],
                    );
                    cb(&uuid, fb_buf.as_ptr(), 4 + copy_len, client.context);
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

    let internal = &mut *(client._internal.as_mut_ptr() as *mut ActionClientInternal);
    let goal_data = core::slice::from_raw_parts(goal, goal_len);

    // C serialize produces [CDR_HEADER(4)][fields] — strip the header.
    let goal_fields = if goal_data.len() > 4 {
        &goal_data[4..]
    } else {
        goal_data
    };

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

    let internal = &mut *(client._internal.as_mut_ptr() as *mut ActionClientInternal);
    let uuid = &*goal_uuid;
    let goal_id = nros_core::GoalId { uuid: uuid.uuid };

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
/// Checks for goal acceptance, feedback, and result. Invokes the
/// registered callbacks. Call this in the spin loop after
/// `nros_executor_spin_some`.
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

    let internal = &mut *(client_ref._internal.as_mut_ptr() as *mut ActionClientInternal);
    let ctx = client_ref.context;

    // Poll goal acceptance reply
    if let Ok(Some(total_len)) = internal.core.try_recv_send_goal_reply() {
        if let Some(cb) = client_ref.goal_response_callback {
            let buf = internal.core.result_buffer_ref();
            let accepted = total_len >= 5 && buf[4] != 0;
            let uuid = nros_goal_uuid_t {
                uuid: {
                    let mut u = [0u8; 16];
                    u[..8].copy_from_slice(&internal.core.goal_counter().to_le_bytes());
                    u
                },
            };
            cb(&uuid, accepted, ctx);
        }
    }

    // Poll feedback
    if let Ok(Some((goal_id, total_len))) = internal.core.try_recv_feedback_raw() {
        if let Some(cb) = client_ref.feedback_callback {
            let buf = internal.core.feedback_buffer_ref();
            let offset = 4 + 16; // CDR header + GoalId
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
    }

    // Poll result reply
    if let Ok(Some(total_len)) = internal.core.try_recv_get_result_reply() {
        if let Some(cb) = client_ref.result_callback {
            let buf = internal.core.result_buffer_ref();
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
                        u[..8].copy_from_slice(&internal.core.goal_counter().to_le_bytes());
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

    // Drop the internal ActionClientCore in place
    core::ptr::drop_in_place(client._internal.as_mut_ptr() as *mut ActionClientInternal);
    client._internal = [0u64; ACTION_CLIENT_INTERNAL_OPAQUE_U64S];
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
        assert_eq!(cli._internal, [0u64; ACTION_CLIENT_INTERNAL_OPAQUE_U64S]);
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
