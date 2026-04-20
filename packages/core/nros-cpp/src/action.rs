//! Action server and client FFI functions for the C++ API.
//!
//! Alloc-free: all internal state is written into caller-provided inline storage.

use core::ffi::{c_char, c_void};

use nros::GoalId;
use nros::cdr::{CDR_HEADER_LEN, strip_cdr_header};
use nros_node::config::DEFAULT_RX_BUF_SIZE;
use nros_node::limits::{MAX_ACTION_NAME_LEN, MAX_TYPE_HASH_LEN, MAX_TYPE_NAME_LEN};

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    NROS_CPP_RET_TIMEOUT, NROS_CPP_RET_TRANSPORT_ERROR, NROS_CPP_RET_TRY_AGAIN, cstr_to_str,
    nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

use crate::{CPP_ACTION_CLIENT_OPAQUE_U64S, CPP_ACTION_SERVER_OPAQUE_U64S};

// ============================================================================
// Action Server
// ============================================================================

/// Goal callback type invoked by the arena goal trampoline.
///
/// Receives raw CDR goal bytes — the C++ header layer deserializes them
/// into the typed `A::Goal` before forwarding to the user's callback.
/// Returns `1` for `AcceptAndExecute`, `2` for `AcceptAndDefer`, `0` for `Reject`.
pub type CppGoalCallback = unsafe extern "C" fn(
    goal_id: *const [u8; 16],
    data: *const u8,
    len: usize,
    ctx: *mut c_void,
) -> i32;

/// Cancel callback type invoked by the arena cancel trampoline.
///
/// Returns `1` for `Accept`, `0` for `Reject`.
pub type CppCancelCallback =
    unsafe extern "C" fn(goal_id: *const [u8; 16], ctx: *mut c_void) -> i32;

/// Internal state for the action server.
///
/// Holds the arena handle, the user-registered callbacks, and just enough
/// metadata for register-after-create. No C++-side goal queue — the arena
/// in `nros-node` owns all lifecycle state.
pub(crate) struct CppActionServer {
    handle: Option<nros_node::ActionServerRawHandle>,
    goal_cb: Option<CppGoalCallback>,
    cancel_cb: Option<CppCancelCallback>,
    cb_ctx: *mut c_void,
    action_name: [u8; MAX_ACTION_NAME_LEN],
    _action_name_len: usize,
    type_name: [u8; MAX_TYPE_NAME_LEN],
    _type_name_len: usize,
    type_hash: [u8; MAX_TYPE_HASH_LEN],
    _type_hash_len: usize,
}

// Compile-time assertion: inline storage must fit CppActionServer.
const _: () = assert!(
    core::mem::size_of::<CppActionServer>()
        <= CPP_ACTION_SERVER_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "CPP_ACTION_SERVER_OPAQUE_U64S too small for CppActionServer"
);

/// Goal callback trampoline — forwards to the user's registered callback
/// (if any) and returns `Reject` otherwise.
///
/// The request bytes arrive as the full CDR payload
/// `[CDR_HDR][seq_prefix][UUID][fields]`; we strip the 24-byte framing so
/// the user callback receives just `[CDR_HDR][fields]` (re-prepended).
///
/// # Safety
/// `context` must point to a valid `CppActionServer`.
unsafe extern "C" fn goal_callback_trampoline(
    goal_id: *const nros::GoalId,
    goal_data: *const u8,
    goal_len: usize,
    context: *mut c_void,
) -> nros::GoalResponse {
    let server = unsafe { &*(context as *const CppActionServer) };
    let Some(cb) = server.goal_cb else {
        return nros::GoalResponse::Reject;
    };

    // Incoming framing: [CDR_HDR][seq_prefix(4)][UUID(16)][goal_fields].
    // User-facing framing: [CDR_HDR][goal_fields].
    let framing_len = CDR_HEADER_LEN + 4 + GoalId::UUID_LEN;
    let slice = unsafe { core::slice::from_raw_parts(goal_data, goal_len) };
    let mut user_buf = [0u8; 512];
    let (ptr, len) = if goal_len > framing_len {
        let fields = &slice[framing_len..];
        user_buf[..CDR_HEADER_LEN].copy_from_slice(&nros::cdr::CDR_LE_HEADER);
        let copy_len = fields.len().min(user_buf.len() - CDR_HEADER_LEN);
        user_buf[CDR_HEADER_LEN..CDR_HEADER_LEN + copy_len].copy_from_slice(&fields[..copy_len]);
        (user_buf.as_ptr(), CDR_HEADER_LEN + copy_len)
    } else {
        (goal_data, goal_len)
    };

    let uuid_ptr = goal_id as *const [u8; 16];
    let resp = unsafe { cb(uuid_ptr, ptr, len, server.cb_ctx) };
    match resp {
        1 => nros::GoalResponse::AcceptAndExecute,
        2 => nros::GoalResponse::AcceptAndDefer,
        _ => nros::GoalResponse::Reject,
    }
}

/// Cancel callback trampoline — forwards to the user's registered
/// callback, defaulting to `Accept` when none is set.
///
/// # Safety
/// `context` must point to a valid `CppActionServer`.
unsafe extern "C" fn cancel_callback_trampoline(
    goal_id: *const nros::GoalId,
    _status: nros::GoalStatus,
    context: *mut c_void,
) -> nros::CancelResponse {
    let server = unsafe { &*(context as *const CppActionServer) };
    let Some(cb) = server.cancel_cb else {
        return nros::CancelResponse::Ok;
    };
    let uuid_ptr = goal_id as *const [u8; 16];
    match unsafe { cb(uuid_ptr, server.cb_ctx) } {
        0 => nros::CancelResponse::Rejected,
        _ => nros::CancelResponse::Ok,
    }
}

/// Create an action server on a node.
///
/// The server auto-accepts incoming goals and buffers them for polling
/// via `nros_cpp_action_server_try_recv_goal()`.
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// 8-byte-aligned buffer of at least `CPP_ACTION_SERVER_OPAQUE_U64S * 8` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_create(
    node: *const nros_cpp_node_t,
    action_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    _qos: nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || action_name.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || storage.is_null()
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

    // Store metadata only — transport handles are created in
    // nros_cpp_action_server_register (called by Node::create_action_server).
    let name_len = act_str.len().min(MAX_ACTION_NAME_LEN - 1);
    let type_len = type_str.len().min(MAX_TYPE_NAME_LEN - 1);
    let hash_len = hash_str.len().min(MAX_TYPE_HASH_LEN - 1);
    let mut server = CppActionServer {
        handle: None,
        goal_cb: None,
        cancel_cb: None,
        cb_ctx: core::ptr::null_mut(),
        action_name: [0u8; MAX_ACTION_NAME_LEN],
        _action_name_len: name_len,
        type_name: [0u8; MAX_TYPE_NAME_LEN],
        _type_name_len: type_len,
        type_hash: [0u8; MAX_TYPE_HASH_LEN],
        _type_hash_len: hash_len,
    };
    server.action_name[..name_len].copy_from_slice(&act_str.as_bytes()[..name_len]);
    server.type_name[..type_len].copy_from_slice(&type_str.as_bytes()[..type_len]);
    server.type_hash[..hash_len].copy_from_slice(&hash_str.as_bytes()[..hash_len]);

    unsafe {
        core::ptr::write(storage as *mut CppActionServer, server);
    }
    NROS_CPP_RET_OK
}

/// Register an action server with the executor (creates transport handles).
///
/// Must be called after `nros_cpp_action_server_create`. Creates the
/// 3 queryables + 2 publishers in the executor context. Separated from
/// create to avoid deadlocks on FreeRTOS QEMU where declaring 5 entities
/// eagerly blocks the session mutex.
///
/// # Safety
/// `storage` must point to a valid `CppActionServer` from create.
/// `executor_handle` must point to a valid `CppContext`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_register(
    storage: *mut c_void,
    executor_handle: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() || executor_handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &mut *(storage as *mut CppActionServer) };
    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };

    let act_str =
        unsafe { core::str::from_utf8_unchecked(&server.action_name[..server._action_name_len]) };
    let type_str =
        unsafe { core::str::from_utf8_unchecked(&server.type_name[..server._type_name_len]) };
    let hash_str =
        unsafe { core::str::from_utf8_unchecked(&server.type_hash[..server._type_hash_len]) };

    match ctx.executor.add_action_server_raw(
        act_str,
        type_str,
        hash_str,
        goal_callback_trampoline,
        cancel_callback_trampoline,
        None, // C++ API runs user callbacks via try_accept_goal, not via the post-accept hook
        storage,
    ) {
        Ok(handle) => {
            server.handle = Some(handle);
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Register callbacks on the action server.
///
/// The goal callback receives raw CDR goal bytes and returns `1`
/// (AcceptAndExecute), `2` (AcceptAndDefer), or `0` (Reject). The cancel
/// callback returns `1` (Accept) or `0` (Reject). Either callback may be
/// null — a null goal callback causes every request to be rejected; a
/// null cancel callback causes every cancel to be accepted. The C++
/// template header translates typed callables into this raw-bytes form.
///
/// # Safety
/// `handle` must be a valid initialized action server storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_set_callbacks(
    handle: *mut c_void,
    goal_cb: Option<CppGoalCallback>,
    cancel_cb: Option<CppCancelCallback>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let server = unsafe { &mut *(handle as *mut CppActionServer) };
    server.goal_cb = goal_cb;
    server.cancel_cb = cancel_cb;
    server.cb_ctx = ctx;
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

/// Iterate over every goal currently live in the arena.
///
/// Calls `visitor(uuid, status, ctx)` for each entry in the
/// arena's `active_goals`. Status is the raw i8 discriminant of
/// `nros_core::GoalStatus` (0 = Unknown, 1 = Accepted, 2 = Executing,
/// 3 = Canceling, 4 = Succeeded, 5 = Canceled, 6 = Aborted). The arena
/// never stores the original goal CDR payload, so only identity + status
/// are forwarded; users needing the goal bytes should stash them in
/// their own `{uuid → state}` table keyed from `set_goal_callback`.
///
/// # Safety
/// `handle` must be a valid `CppActionServer` storage pointer.
/// `executor_handle` must point to a valid `CppContext`.
/// `visitor` must be a valid function pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_for_each_active_goal(
    handle: *mut c_void,
    executor_handle: *mut c_void,
    visitor: Option<unsafe extern "C" fn(goal_id: *const [u8; 16], status: i8, ctx: *mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if handle.is_null() || executor_handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(visitor) = visitor else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let server = unsafe { &*(handle as *const CppActionServer) };
    let executor_ctx = unsafe { &*(executor_handle as *const CppContext) };
    let arena_handle = match &server.handle {
        Some(h) => *h,
        None => return NROS_CPP_RET_ERROR,
    };
    arena_handle.for_each_active_goal(&executor_ctx.executor, |g| unsafe {
        let uuid_ptr: *const [u8; 16] = &g.goal_id.uuid;
        visitor(uuid_ptr, g.status as i8, ctx);
    });
    NROS_CPP_RET_OK
}

/// Destroy an action server (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized action server storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut CppActionServer);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Action Client
// ============================================================================

/// Internal state for the action client.
/// C++ action client callback function pointers (freestanding C++14).
#[repr(C)]
pub(crate) struct CppActionClientCallbacks {
    pub goal_response:
        Option<unsafe extern "C" fn(accepted: bool, goal_id: *const [u8; 16], ctx: *mut c_void)>,
    pub feedback: Option<
        unsafe extern "C" fn(
            goal_id: *const [u8; 16],
            data: *const u8,
            len: usize,
            ctx: *mut c_void,
        ),
    >,
    pub result: Option<
        unsafe extern "C" fn(
            goal_id: *const [u8; 16],
            status: i32,
            data: *const u8,
            len: usize,
            ctx: *mut c_void,
        ),
    >,
    pub context: *mut c_void,
}

impl Default for CppActionClientCallbacks {
    fn default() -> Self {
        Self {
            goal_response: None,
            feedback: None,
            result: None,
            context: core::ptr::null_mut(),
        }
    }
}

/// Internal state for the C++ action client.
///
/// Lightweight — the `ActionClientCore` lives in the executor's arena.
/// This struct stores the arena entry index, executor pointer, and callbacks.
pub(crate) struct CppActionClient {
    callbacks: CppActionClientCallbacks,
    arena_entry_index: i32,
    executor_ptr: *mut c_void,
    action_name: [u8; MAX_ACTION_NAME_LEN],
    _action_name_len: usize,
}

/// Get a mutable reference to an action client's core in the executor arena.
///
/// # Safety
/// `executor_ptr` must point to a valid `CppContext`.
unsafe fn cpp_arena_core_mut<'a>(
    arena_entry_index: i32,
    executor_ptr: *mut c_void,
) -> Option<&'a mut nros_node::ActionClientCore> {
    if arena_entry_index < 0 || executor_ptr.is_null() {
        return None;
    }
    unsafe {
        let ctx = &mut *(executor_ptr as *mut CppContext);
        ctx.executor
            .action_client_core_mut(arena_entry_index as usize)
    }
}

// Compile-time assertion: inline storage must fit CppActionClient.
const _: () = assert!(
    core::mem::size_of::<CppActionClient>()
        <= CPP_ACTION_CLIENT_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "CPP_ACTION_CLIENT_OPAQUE_U64S too small for CppActionClient"
);

// C++ action client callback trampolines for the arena entry.
// `context` is the CppActionClient storage pointer.
unsafe extern "C" fn cpp_goal_response_trampoline(
    goal_id: *const nros::GoalId,
    accepted: bool,
    context: *mut c_void,
) {
    let client = unsafe { &*(context as *const CppActionClient) };
    if let Some(cb) = client.callbacks.goal_response {
        unsafe { cb(accepted, &(*goal_id).uuid, client.callbacks.context) };
    }
}

unsafe extern "C" fn cpp_feedback_trampoline(
    goal_id: *const nros::GoalId,
    feedback_data: *const u8,
    feedback_len: usize,
    context: *mut c_void,
) {
    let client = unsafe { &*(context as *const CppActionClient) };
    if let Some(cb) = client.callbacks.feedback {
        unsafe {
            cb(
                &(*goal_id).uuid,
                feedback_data,
                feedback_len,
                client.callbacks.context,
            )
        };
    }
}

/// Stash for result data captured by the trampoline.
/// Used by `nros_cpp_action_client_try_recv_result` to retrieve
/// results that were consumed by the executor's auto-dispatch.
static mut RESULT_STASH_LEN: i32 = -1; // -1 = empty, >= 0 = data length
static mut RESULT_STASH: [u8; DEFAULT_RX_BUF_SIZE] = [0u8; DEFAULT_RX_BUF_SIZE];

unsafe extern "C" fn cpp_result_trampoline(
    goal_id: *const nros::GoalId,
    status: nros::GoalStatus,
    result_data: *const u8,
    result_len: usize,
    context: *mut c_void,
) {
    // Always stash the result for Future::wait polling
    unsafe {
        let copy_len = result_len.min(DEFAULT_RX_BUF_SIZE);
        core::ptr::copy_nonoverlapping(
            result_data,
            core::ptr::addr_of_mut!(RESULT_STASH) as *mut u8,
            copy_len,
        );
        core::ptr::write(core::ptr::addr_of_mut!(RESULT_STASH_LEN), copy_len as i32);
    }

    // Also forward to user callback if set
    let client = unsafe { &*(context as *const CppActionClient) };
    if let Some(cb) = client.callbacks.result {
        let s = match status {
            nros::GoalStatus::Succeeded => 4i32,
            nros::GoalStatus::Canceled => 5,
            nros::GoalStatus::Aborted => 6,
            _ => 0,
        };
        unsafe {
            cb(
                &(*goal_id).uuid,
                s,
                result_data,
                result_len,
                client.callbacks.context,
            )
        };
    }
}

/// Create an action client on a node.
///
/// # Safety
/// All pointer parameters must be valid. `storage` must point to an
/// 8-byte-aligned buffer of at least `CPP_ACTION_CLIENT_OPAQUE_U64S * 8` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_create(
    node: *const nros_cpp_node_t,
    action_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    _qos: nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null()
        || action_name.is_null()
        || type_name.is_null()
        || type_hash.is_null()
        || storage.is_null()
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

    // Register with executor — creates the ONLY ActionClientCore in the arena.
    // Trampolines read from CppActionClient.callbacks (set later via set_callbacks).
    let handle = match ctx.executor.add_action_client_raw(
        act_str,
        type_str,
        hash_str,
        Some(cpp_goal_response_trampoline),
        Some(cpp_feedback_trampoline),
        Some(cpp_result_trampoline),
        storage, // context = CppActionClient pointer
    ) {
        Ok(h) => h,
        Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
    };

    let name_len = act_str.len().min(MAX_ACTION_NAME_LEN - 1);
    let mut client = CppActionClient {
        callbacks: CppActionClientCallbacks::default(),
        arena_entry_index: handle.entry_index() as i32,
        executor_ptr: node_ref.executor,
        action_name: [0u8; MAX_ACTION_NAME_LEN],
        _action_name_len: name_len,
    };
    client.action_name[..name_len].copy_from_slice(&act_str.as_bytes()[..name_len]);

    // Write directly into caller-provided storage — no heap allocation.
    unsafe {
        core::ptr::write(storage as *mut CppActionClient, client);
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
#[allow(static_mut_refs)]
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

    // C++ ffi_serialize produces [CDR_HEADER][fields], but send_goal_blocking
    // expects raw fields only (it adds its own CDR header + GoalId).
    let goal_fields = strip_cdr_header(goal_data);

    // Send goal via arena core (non-blocking)
    let core = match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
        Some(c) => c,
        None => return NROS_CPP_RET_ERROR,
    };
    let goal_id = match core.send_goal_raw(goal_fields) {
        Ok(id) => id,
        Err(_) => return NROS_CPP_RET_ERROR,
    };
    unsafe {
        *goal_id_out = goal_id.uuid;
    }

    // Use a flag-based approach: install a temporary goal_response callback
    // that sets a local flag. The arena's action_client_raw_try_process fires
    // the trampoline during spin_once, which reads client.callbacks and
    // dispatches to the user's callback (or our temporary one).
    static mut BLOCKING_ACCEPTED: i32 = -1; // -1=pending, 0=rejected, 1=accepted
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(BLOCKING_ACCEPTED), -1i32);
    }

    // Save original callback and install temporary one
    let orig_cb = client.callbacks.goal_response;
    let orig_ctx = client.callbacks.context;
    unsafe extern "C" fn blocking_goal_cb(
        _accepted: bool,
        _goal_id: *const [u8; 16],
        _ctx: *mut c_void,
    ) {
        unsafe {
            core::ptr::write(core::ptr::addr_of_mut!(BLOCKING_ACCEPTED), if _accepted { 1i32 } else { 0i32 });
        }
    }
    client.callbacks.goal_response = Some(blocking_goal_cb);
    client.callbacks.context = core::ptr::null_mut();

    // Spin executor until flag set or timeout (~10s = 1000 × 10ms)
    let ctx = unsafe { &mut *(client.executor_ptr as *mut CppContext) };
    for _ in 0..1000 {
        let _ = ctx.executor.spin_once(10);
        let flag = unsafe { core::ptr::read(core::ptr::addr_of!(BLOCKING_ACCEPTED)) };
        if flag >= 0 {
            // Restore original callback
            client.callbacks.goal_response = orig_cb;
            client.callbacks.context = orig_ctx;
            return if flag == 1 {
                NROS_CPP_RET_OK
            } else {
                NROS_CPP_RET_ERROR
            };
        }
    }
    // Restore original callback on timeout
    client.callbacks.goal_response = orig_cb;
    client.callbacks.context = orig_ctx;
    NROS_CPP_RET_TIMEOUT
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
#[allow(static_mut_refs)]
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
    let _ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };

    // Send get_result request via arena core (non-blocking)
    {
        let core =
            match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
                Some(c) => c,
                None => return NROS_CPP_RET_ERROR,
            };
        if core.send_get_result_request(&id).is_err() {
            return NROS_CPP_RET_ERROR;
        }
    }

    // Flag-based: install temporary result callback, spin until flag set.
    static mut BLOCKING_RESULT_LEN: i32 = -1; // -1=pending, >=0=length
    static mut BLOCKING_RESULT_STATUS: i32 = 0;
    static mut BLOCKING_RESULT_BUF: [u8; DEFAULT_RX_BUF_SIZE] = [0u8; DEFAULT_RX_BUF_SIZE];
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(BLOCKING_RESULT_LEN), -1i32);
        core::ptr::write(core::ptr::addr_of_mut!(BLOCKING_RESULT_STATUS), 0i32);
    }

    let orig_cb = client.callbacks.result;
    let orig_ctx = client.callbacks.context;
    unsafe extern "C" fn blocking_result_cb(
        _goal_id: *const [u8; 16],
        status: i32,
        data: *const u8,
        len: usize,
        _ctx: *mut c_void,
    ) {
        unsafe {
            core::ptr::write(core::ptr::addr_of_mut!(BLOCKING_RESULT_STATUS), status);
            let copy_len = len.min(DEFAULT_RX_BUF_SIZE);
            core::ptr::copy_nonoverlapping(
                data,
                core::ptr::addr_of_mut!(BLOCKING_RESULT_BUF) as *mut u8,
                copy_len,
            );
            core::ptr::write(core::ptr::addr_of_mut!(BLOCKING_RESULT_LEN), copy_len as i32);
        }
    }
    client.callbacks.result = Some(blocking_result_cb);
    client.callbacks.context = core::ptr::null_mut();

    // Spin executor until flag set or timeout (~10s = 1000 × 10ms)
    let ctx = unsafe { &mut *(client.executor_ptr as *mut CppContext) };
    for _ in 0..1000 {
        let _ = ctx.executor.spin_once(10);
        let rlen = unsafe { core::ptr::read(core::ptr::addr_of!(BLOCKING_RESULT_LEN)) };
        if rlen >= 0 {
            client.callbacks.result = orig_cb;
            client.callbacks.context = orig_ctx;
            let data_len = rlen as usize;
            if data_len <= result_buf_len {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        core::ptr::addr_of!(BLOCKING_RESULT_BUF) as *const u8,
                        result_buf,
                        data_len,
                    );
                    *result_len = data_len;
                }
                return NROS_CPP_RET_OK;
            }
            return NROS_CPP_RET_ERROR;
        }
    }
    client.callbacks.result = orig_cb;
    client.callbacks.context = orig_ctx;
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

    let core = match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
        Some(c) => c,
        None => {
            unsafe {
                *feedback_len = 0;
            }
            return NROS_CPP_RET_OK;
        }
    };

    match core.try_recv_feedback_raw() {
        Ok(Some((_goal_id, total_len))) => {
            // Feedback buffer layout: [CDR_HEADER][UUID][feedback_fields]
            let buf = core.feedback_buffer_ref();
            let offset = CDR_HEADER_LEN + GoalId::UUID_LEN;
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

/// Try to receive the goal acceptance response (non-blocking).
///
/// Returns `NROS_CPP_RET_OK` with serialized `GoalAccept` data if ready,
/// `NROS_CPP_RET_TRY_AGAIN` if not yet available.
///
/// Output layout: `[goal_id: 16 bytes][accepted: 1 byte]` (17 bytes total).
///
/// Used by C++ `Future<GoalAccept>` via the `TryRecvFn` interface.
///
/// # Safety
/// All pointers must be valid. `out_data` must point to `out_capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_goal_response(
    handle: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || out_data.is_null() || out_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let core = match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
        Some(c) => c,
        None => {
            unsafe { *out_len = 0 };
            return NROS_CPP_RET_TRY_AGAIN;
        }
    };

    match core.try_recv_send_goal_reply() {
        Ok(Some(total_len)) => {
            // The reply buffer contains: CDR header (4) + accepted byte (1) + ...
            // We produce: goal_id (16) + accepted (1) = 17 bytes
            let needed = 17usize;
            if needed > out_capacity {
                unsafe { *out_len = needed };
                return NROS_CPP_RET_ERROR;
            }
            let buf = core.result_buffer_ref();
            let accepted: u8 = if total_len >= 5 && buf[4] != 0 { 1 } else { 0 };
            // Fill goal_id from the counter (same logic as poll())
            let counter = core.goal_counter();
            let mut uuid = [0u8; 16];
            uuid[..8].copy_from_slice(&counter.to_le_bytes());
            unsafe {
                core::ptr::copy_nonoverlapping(uuid.as_ptr(), out_data, 16);
                *out_data.add(16) = accepted;
                *out_len = needed;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe { *out_len = 0 };
            NROS_CPP_RET_TRY_AGAIN
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Try to receive the result for a pending get_result request (non-blocking).
///
/// Returns `NROS_CPP_RET_OK` with result data if ready,
/// `NROS_CPP_RET_TRY_AGAIN` if not yet available.
///
/// Output layout: CDR-serialized result fields (same as `get_result` output).
///
/// Used by C++ `Future<ResultType>` via the `TryRecvFn` interface.
///
/// # Safety
/// All pointers must be valid. `out_data` must point to `out_capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_result(
    handle: *mut c_void,
    out_data: *mut u8,
    out_capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || out_data.is_null() || out_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    // Check the result stash (filled by cpp_result_trampoline during spin_once).
    // The executor's action_client_raw_try_process consumes the reply from the
    // core and fires the trampoline, which stashes the data here. We can't call
    // core.try_recv_get_result_reply() because the data is already consumed.
    let stash_len = unsafe { core::ptr::read(core::ptr::addr_of!(RESULT_STASH_LEN)) };
    if stash_len >= 0 {
        let data_len = stash_len as usize;
        unsafe { core::ptr::write(core::ptr::addr_of_mut!(RESULT_STASH_LEN), -1i32) };

        // The stash contains raw result fields (no CDR header) from the trampoline.
        // ffi_deserialize expects CDR-encoded data, so prepend a CDR header.
        let total_len = 4 + data_len; // CDR header (4) + result fields
        if total_len > out_capacity {
            unsafe { *out_len = total_len };
            return NROS_CPP_RET_ERROR;
        }
        unsafe {
            // CDR header: little-endian, no options
            let cdr_header: [u8; 4] = [0x00, 0x01, 0x00, 0x00];
            core::ptr::copy_nonoverlapping(cdr_header.as_ptr(), out_data, 4);
            core::ptr::copy_nonoverlapping(
                core::ptr::addr_of!(RESULT_STASH) as *const u8,
                out_data.add(4),
                data_len,
            );
            *out_len = total_len;
        }
        return NROS_CPP_RET_OK;
    }

    // Fallback: check the core directly (in case the trampoline wasn't fired yet)
    let _client = unsafe { &mut *(handle as *mut CppActionClient) };
    let core = match unsafe { cpp_arena_core_mut(_client.arena_entry_index, _client.executor_ptr) }
    {
        Some(c) => c,
        None => {
            unsafe { *out_len = 0 };
            return NROS_CPP_RET_TRY_AGAIN;
        }
    };

    match core.try_recv_get_result_reply() {
        Ok(Some(total_len)) => {
            let result_offset = 5usize;
            if total_len <= result_offset {
                unsafe { *out_len = 0 };
                return NROS_CPP_RET_OK;
            }
            let data_len = total_len - result_offset;
            if data_len > out_capacity {
                unsafe { *out_len = data_len };
                return NROS_CPP_RET_ERROR;
            }
            let buf = core.result_buffer_ref();
            unsafe {
                core::ptr::copy_nonoverlapping(
                    buf[result_offset..total_len].as_ptr(),
                    out_data,
                    data_len,
                );
                *out_len = data_len;
            }
            NROS_CPP_RET_OK
        }
        Ok(None) => {
            unsafe { *out_len = 0 };
            NROS_CPP_RET_TRY_AGAIN
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Destroy an action client (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized action client storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut CppActionClient);
    }
    NROS_CPP_RET_OK
}

// ============================================================================
// Async (non-blocking) Action Client FFI
// ============================================================================

/// Send a goal asynchronously (non-blocking).
///
/// Uses `send_goal_raw` (zpico_get_start) instead of `send_goal_blocking`.
/// The goal response arrives via the callback registered with
/// `nros_cpp_action_client_register_async`, invoked during `spin_once`.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_send_goal_async(
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

    // Strip CDR header (same as blocking variant)
    let goal_fields = strip_cdr_header(goal_data);

    let core = match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
        Some(c) => c,
        None => return NROS_CPP_RET_ERROR,
    };

    // Non-blocking: uses zpico_get_start internally
    match core.send_goal_raw(goal_fields) {
        Ok(goal_id) => {
            unsafe {
                *goal_id_out = goal_id.uuid;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Request a goal result asynchronously (non-blocking).
///
/// Uses `send_get_result_request` (zpico_get_start) instead of
/// `get_result_blocking`. The result arrives via the result callback
/// registered with `nros_cpp_action_client_register_async`.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_get_result_async(
    handle: *mut c_void,
    goal_id: *const [u8; 16],
) -> nros_cpp_ret_t {
    if handle.is_null() || goal_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };

    // Reset result stash before sending (so try_recv_result starts clean)
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(RESULT_STASH_LEN), -1i32) };

    let core = match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
        Some(c) => c,
        None => return NROS_CPP_RET_ERROR,
    };

    match core.send_get_result_request(&id) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Register async callbacks on the action client.
///
/// # Safety
/// `handle` must be a valid action client storage. Function pointers
/// may be null (no callback for that event).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_set_callbacks(
    handle: *mut c_void,
    goal_response: Option<unsafe extern "C" fn(bool, *const [u8; 16], *mut c_void)>,
    feedback: Option<unsafe extern "C" fn(*const [u8; 16], *const u8, usize, *mut c_void)>,
    result: Option<unsafe extern "C" fn(*const [u8; 16], i32, *const u8, usize, *mut c_void)>,
    context: *mut c_void,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    client.callbacks.goal_response = goal_response;
    client.callbacks.feedback = feedback;
    client.callbacks.result = result;
    client.callbacks.context = context;
    NROS_CPP_RET_OK
}

/// Poll action client for pending replies (non-blocking).
///
/// Checks for goal acceptance reply, feedback, and result reply.
/// Invokes the corresponding callbacks registered via
/// `nros_cpp_action_client_set_callbacks`.
///
/// Call this in the spin loop after `nros_cpp_spin_once`.
///
/// # Safety
/// `handle` must be a valid action client storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_poll(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };

    // Read callbacks before borrowing the arena core (avoids borrow conflict)
    let goal_response_cb = client.callbacks.goal_response;
    let feedback_cb = client.callbacks.feedback;
    let result_cb = client.callbacks.result;
    let ctx = client.callbacks.context;
    let idx = client.arena_entry_index;
    let eptr = client.executor_ptr;

    let make_uuid = |counter: u64| -> [u8; 16] {
        let mut u = [0u8; 16];
        u[..8].copy_from_slice(&counter.to_le_bytes());
        u
    };

    let core = match unsafe { cpp_arena_core_mut(idx, eptr) } {
        Some(c) => c,
        None => return NROS_CPP_RET_OK,
    };

    // Poll goal acceptance reply
    if let Ok(Some(total_len)) = core.try_recv_send_goal_reply()
        && let Some(cb) = goal_response_cb
    {
        let buf = core.result_buffer_ref();
        let accepted = total_len >= 5 && buf[4] != 0;
        let uuid = make_uuid(core.goal_counter());
        unsafe { cb(accepted, &uuid, ctx) };
    }

    // Poll feedback
    if let Ok(Some((goal_id, total_len))) = core.try_recv_feedback_raw()
        && let Some(cb) = feedback_cb
    {
        // [CDR_HEADER][UUID][feedback_fields]
        let offset = CDR_HEADER_LEN + GoalId::UUID_LEN;
        if total_len > offset {
            let buf = core.feedback_buffer_ref();
            unsafe {
                cb(
                    &goal_id.uuid,
                    buf[offset..total_len].as_ptr(),
                    total_len - offset,
                    ctx,
                );
            }
        }
    }

    // Poll result reply
    if let Ok(Some(total_len)) = core.try_recv_get_result_reply()
        && let Some(cb) = result_cb
        && total_len >= 5
    {
        let buf = core.result_buffer_ref();
        let status = buf[4] as i32;
        let result_offset = 5;
        let uuid = make_uuid(core.goal_counter());
        unsafe {
            cb(
                &uuid,
                status,
                buf[result_offset..total_len].as_ptr(),
                total_len - result_offset,
                ctx,
            );
        }
    }

    NROS_CPP_RET_OK
}
