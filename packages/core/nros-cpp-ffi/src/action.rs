//! Action server and client FFI functions for the C++ API.

use core::ffi::{c_char, c_void};

use nros_rmw::{ActionInfo, QosSettings, ServiceInfo, Session, TopicInfo};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TIMEOUT, NROS_CPP_RET_TRANSPORT_ERROR, cstr_to_str, nros_cpp_node_t,
    nros_cpp_qos_t, nros_cpp_ret_t,
};

/// Buffer size for action messages.
const ACTION_BUF_SIZE: usize = 1024;

// ============================================================================
// Action Server
// ============================================================================

/// Maximum number of pending (unpolled) goal requests.
const MAX_PENDING_GOALS: usize = 4;

/// A pending goal request buffered by the goal callback.
struct PendingGoal {
    goal_id: nros::GoalId,
    data: [u8; ACTION_BUF_SIZE],
    data_len: usize,
    occupied: bool,
}

impl Default for PendingGoal {
    fn default() -> Self {
        Self {
            goal_id: nros::GoalId::default(),
            data: [0u8; ACTION_BUF_SIZE],
            data_len: 0,
            occupied: false,
        }
    }
}

/// Internal state for the action server.
///
/// The `pending` array is filled by the goal callback trampoline during `spin_once()`,
/// and drained by `try_recv_goal()`.
struct CppActionServer {
    handle: Option<nros_node::ActionServerRawHandle>,
    pending: [PendingGoal; MAX_PENDING_GOALS],
    action_name: [u8; 256],
    _action_name_len: usize,
}

/// Goal callback trampoline — auto-accepts goals and buffers them for polling.
///
/// # Safety
/// `context` must point to a valid `CppActionServer`.
unsafe extern "C" fn goal_callback_trampoline(
    goal_id: *const nros::GoalId,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut c_void,
) -> nros::GoalResponse {
    let server = unsafe { &mut *(context as *mut CppActionServer) };
    let id = unsafe { *goal_id };

    // Find an empty slot in the pending queue
    for slot in &mut server.pending {
        if !slot.occupied {
            slot.goal_id = id;
            let copy_len = goal_len.min(ACTION_BUF_SIZE);
            unsafe {
                core::ptr::copy_nonoverlapping(goal_data, slot.data.as_mut_ptr(), copy_len);
            }
            slot.data_len = copy_len;
            slot.occupied = true;
            return nros::GoalResponse::AcceptAndExecute;
        }
    }

    // No room — reject the goal
    nros::GoalResponse::Reject
}

/// Cancel callback trampoline — accepts all cancel requests.
///
/// # Safety
/// `context` must point to a valid `CppActionServer`.
unsafe extern "C" fn cancel_callback_trampoline(
    _goal_id: *const nros::GoalId,
    _status: nros::GoalStatus,
    _context: *mut c_void,
) -> nros::CancelResponse {
    nros::CancelResponse::Ok
}

/// Create an action server on a node.
///
/// The server auto-accepts incoming goals and buffers them for polling
/// via `nros_cpp_action_server_try_recv_goal()`.
///
/// # Safety
/// All pointer parameters must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_create(
    node: *const nros_cpp_node_t,
    action_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    _qos: nros_cpp_qos_t,
    out_handle: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || action_name.is_null()
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

    let act_str = match unsafe { cstr_to_str(action_name) } {
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

    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };

    // Allocate the server struct first so we can pass its address as context
    let mut server = alloc::boxed::Box::new(CppActionServer {
        handle: None,
        pending: Default::default(),
        action_name: [0u8; 256],
        _action_name_len: act_str.len().min(255),
    });
    server.action_name[..server._action_name_len]
        .copy_from_slice(&act_str.as_bytes()[..server._action_name_len]);

    let server_ptr = &mut *server as *mut CppActionServer as *mut c_void;

    match ctx.executor.add_action_server_raw(
        act_str,
        type_str,
        hash_str,
        goal_callback_trampoline,
        cancel_callback_trampoline,
        server_ptr,
    ) {
        Ok(handle) => {
            server.handle = Some(handle);
            unsafe {
                *out_handle = alloc::boxed::Box::into_raw(server) as *mut c_void;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Try to receive a pending goal request (non-blocking).
///
/// Goals are auto-accepted during `spin_once()`. This function returns
/// the next buffered goal request.
///
/// # Parameters
/// * `handle` — Action server handle.
/// * `goal_buf` — Buffer to receive CDR-serialized goal data.
/// * `buf_len` — Size of the goal buffer.
/// * `goal_len` — Receives the actual goal data length (0 if no pending goal).
/// * `goal_id_out` — Receives the 16-byte goal UUID.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_try_recv_goal(
    handle: *mut c_void,
    goal_buf: *mut u8,
    buf_len: usize,
    goal_len: *mut usize,
    goal_id_out: *mut [u8; 16],
) -> nros_cpp_ret_t {
    if handle.is_null() || goal_buf.is_null() || goal_len.is_null() || goal_id_out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &mut *(handle as *mut CppActionServer) };

    // Find and consume the first pending goal
    for slot in &mut server.pending {
        if slot.occupied {
            let len = slot.data_len;
            if len <= buf_len {
                unsafe {
                    core::ptr::copy_nonoverlapping(slot.data.as_ptr(), goal_buf, len);
                    *goal_len = len;
                    *goal_id_out = slot.goal_id.uuid;
                }
                slot.occupied = false;
                return NROS_CPP_RET_OK;
            } else {
                unsafe {
                    *goal_len = len;
                }
                return NROS_CPP_RET_ERROR;
            }
        }
    }

    // No pending goals
    unsafe {
        *goal_len = 0;
    }
    NROS_CPP_RET_OK
}

/// Publish feedback for an active goal.
///
/// # Parameters
/// * `handle` — Action server handle.
/// * `executor_handle` — Executor handle from `nros_cpp_init()`.
/// * `goal_id` — 16-byte goal UUID.
/// * `feedback_buf` — CDR-serialized feedback data.
/// * `feedback_len` — Length of feedback data.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_publish_feedback(
    handle: *mut c_void,
    executor_handle: *mut c_void,
    goal_id: *const [u8; 16],
    feedback_buf: *const u8,
    feedback_len: usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || executor_handle.is_null() || goal_id.is_null() || feedback_buf.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &*(handle as *const CppActionServer) };
    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };
    let data = unsafe { core::slice::from_raw_parts(feedback_buf, feedback_len) };

    let h = match &server.handle {
        Some(h) => h,
        None => return NROS_CPP_RET_ERROR,
    };
    match h.publish_feedback_raw(&mut ctx.executor, &id, data) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Complete a goal with a result.
///
/// # Parameters
/// * `handle` — Action server handle.
/// * `executor_handle` — Executor handle from `nros_cpp_init()`.
/// * `goal_id` — 16-byte goal UUID.
/// * `result_buf` — CDR-serialized result data.
/// * `result_len` — Length of result data.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_complete_goal(
    handle: *mut c_void,
    executor_handle: *mut c_void,
    goal_id: *const [u8; 16],
    result_buf: *const u8,
    result_len: usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || executor_handle.is_null() || goal_id.is_null() || result_buf.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &*(handle as *const CppActionServer) };
    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };
    let data = unsafe { core::slice::from_raw_parts(result_buf, result_len) };

    let h = match &server.handle {
        Some(h) => h,
        None => return NROS_CPP_RET_ERROR,
    };
    h.complete_goal_raw(&mut ctx.executor, &id, nros::GoalStatus::Succeeded, data);
    NROS_CPP_RET_OK
}

/// Destroy an action server and free its resources.
///
/// # Safety
/// `handle` must be a valid action server handle, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_destroy(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        let _server = alloc::boxed::Box::from_raw(handle as *mut CppActionServer);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Action Client
// ============================================================================

/// Internal state for the action client.
struct CppActionClient {
    core: nros_node::ActionClientCore<
        nros::internals::RmwServiceClient,
        nros::internals::RmwSubscriber,
        ACTION_BUF_SIZE,
        ACTION_BUF_SIZE,
        ACTION_BUF_SIZE,
    >,
    action_name: [u8; 256],
    _action_name_len: usize,
}

/// Create an action client on a node.
///
/// # Safety
/// All pointer parameters must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_create(
    node: *const nros_cpp_node_t,
    action_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    _qos: nros_cpp_qos_t,
    out_handle: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || action_name.is_null()
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

    let act_str = match unsafe { cstr_to_str(action_name) } {
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

    let ctx = unsafe { &mut *(node_ref.executor as *mut CppContext) };
    let action_info = ActionInfo::new(act_str, type_str, hash_str).with_domain(ctx.domain_id);

    // Create send_goal service client
    let send_goal_key = action_info.send_goal_key::<256>();
    let send_goal_info = ServiceInfo::new(&send_goal_key, type_str, hash_str).with_domain(0);
    let send_goal_client = match ctx
        .executor
        .session_mut()
        .create_service_client(&send_goal_info)
    {
        Ok(c) => c,
        Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
    };

    // Create cancel_goal service client
    let cancel_goal_key = action_info.cancel_goal_key::<256>();
    let cancel_goal_info = ServiceInfo::new(
        &cancel_goal_key,
        "action_msgs::srv::dds_::CancelGoal_",
        hash_str,
    )
    .with_domain(0);
    let cancel_goal_client = match ctx
        .executor
        .session_mut()
        .create_service_client(&cancel_goal_info)
    {
        Ok(c) => c,
        Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
    };

    // Create get_result service client
    let get_result_key = action_info.get_result_key::<256>();
    let get_result_info = ServiceInfo::new(&get_result_key, type_str, hash_str).with_domain(0);
    let get_result_client = match ctx
        .executor
        .session_mut()
        .create_service_client(&get_result_info)
    {
        Ok(c) => c,
        Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
    };

    // Create feedback subscriber (best-effort QoS)
    let feedback_key = action_info.feedback_key::<256>();
    let feedback_topic = TopicInfo::new(&feedback_key, type_str, hash_str).with_domain(0);
    let feedback_sub = match ctx
        .executor
        .session_mut()
        .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
    {
        Ok(s) => s,
        Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
    };

    let core = nros_node::ActionClientCore::new(
        send_goal_client,
        cancel_goal_client,
        get_result_client,
        feedback_sub,
    );

    let mut client = CppActionClient {
        core,
        action_name: [0u8; 256],
        _action_name_len: act_str.len().min(255),
    };
    client.action_name[..client._action_name_len]
        .copy_from_slice(&act_str.as_bytes()[..client._action_name_len]);

    let boxed = alloc::boxed::Box::new(client);
    unsafe {
        *out_handle = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
    }
    NROS_CPP_RET_OK
}

/// Send a goal and receive the generated goal UUID.
///
/// # Parameters
/// * `handle` — Action client handle.
/// * `goal_buf` — CDR-serialized goal data.
/// * `goal_len` — Length of goal data.
/// * `goal_id_out` — Receives the 16-byte goal UUID.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_send_goal(
    handle: *mut c_void,
    goal_buf: *const u8,
    goal_len: usize,
    goal_id_out: *mut [u8; 16],
) -> nros_cpp_ret_t {
    if handle.is_null() || goal_buf.is_null() || goal_id_out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let goal_data = unsafe { core::slice::from_raw_parts(goal_buf, goal_len) };

    match client.core.send_goal_raw(goal_data) {
        Ok(goal_id) => {
            unsafe {
                *goal_id_out = goal_id.uuid;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Get the result for a goal (blocking with timeout).
///
/// Sends a get_result request and polls for the reply.
///
/// # Parameters
/// * `handle` — Action client handle.
/// * `executor_handle` — Executor handle for spin_once during polling.
/// * `goal_id` — 16-byte goal UUID.
/// * `result_buf` — Buffer for CDR-serialized result data.
/// * `result_buf_len` — Size of result buffer.
/// * `result_len` — Receives actual result data length.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_get_result(
    handle: *mut c_void,
    executor_handle: *mut c_void,
    goal_id: *const [u8; 16],
    result_buf: *mut u8,
    result_buf_len: usize,
    result_len: *mut usize,
) -> nros_cpp_ret_t {
    if handle.is_null()
        || executor_handle.is_null()
        || goal_id.is_null()
        || result_buf.is_null()
        || result_len.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };

    // Send get_result request
    if client.core.send_get_result_request(&id).is_err() {
        return NROS_CPP_RET_ERROR;
    }

    // Poll for reply with timeout (~3000 iterations * 1ms spin_once)
    for _ in 0..3000 {
        let _ = ctx.executor.spin_once(1);
        match client.core.try_recv_get_result_reply() {
            Ok(Some(total_len)) => {
                // Result buffer layout: CDR header (4) + status (1) + padding (3) + result
                let buf = client.core.result_buffer_ref();
                if total_len >= 8 {
                    let result_data = &buf[8..total_len];
                    if result_data.len() <= result_buf_len {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                result_data.as_ptr(),
                                result_buf,
                                result_data.len(),
                            );
                            *result_len = result_data.len();
                        }
                        return NROS_CPP_RET_OK;
                    }
                }
                return NROS_CPP_RET_ERROR;
            }
            Ok(None) => continue,
            Err(_) => return NROS_CPP_RET_ERROR,
        }
    }

    NROS_CPP_RET_TIMEOUT
}

/// Try to receive feedback (non-blocking).
///
/// # Parameters
/// * `handle` — Action client handle.
/// * `feedback_buf` — Buffer for CDR-serialized feedback data.
/// * `buf_len` — Size of feedback buffer.
/// * `feedback_len` — Receives actual feedback data length (0 if none available).
///
/// # Returns
/// `NROS_CPP_RET_OK` on success (check `*feedback_len > 0` for data).
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_feedback(
    handle: *mut c_void,
    feedback_buf: *mut u8,
    buf_len: usize,
    feedback_len: *mut usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || feedback_buf.is_null() || feedback_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };

    match client.core.try_recv_feedback_raw() {
        Ok(Some((_goal_id, total_len))) => {
            // Feedback buffer layout: CDR header (4) + GoalId (16) + feedback data
            let buf = client.core.feedback_buffer_ref();
            let offset = 4 + 16; // CDR header + UUID
            if total_len > offset {
                let data = &buf[offset..total_len];
                if data.len() <= buf_len {
                    unsafe {
                        core::ptr::copy_nonoverlapping(data.as_ptr(), feedback_buf, data.len());
                        *feedback_len = data.len();
                    }
                    return NROS_CPP_RET_OK;
                }
            }
            unsafe {
                *feedback_len = 0;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe {
                *feedback_len = 0;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Destroy an action client and free its resources.
///
/// # Safety
/// `handle` must be a valid action client handle, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_destroy(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        let _client = alloc::boxed::Box::from_raw(handle as *mut CppActionClient);
    }
    NROS_CPP_RET_OK
}
