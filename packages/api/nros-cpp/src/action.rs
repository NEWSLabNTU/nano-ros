//! Action server and client FFI functions for the C++ API.
//!
//! Alloc-free: all internal state is written into caller-provided inline storage.
//!
//! Issue 0436 — user-supplied executor handles are tag-validated via
//! `cpp_ctx_checked`. Handles reached through an already-validated node/client
//! (`node_ref.executor`, `client.executor_ptr`) inherit that provenance and stay
//! direct casts.

use core::ffi::{c_char, c_void};

use nros::{
    GoalId,
    cdr::{CDR_HEADER_LEN, strip_cdr_header},
};
use nros_node::{action_frame, config::DEFAULT_RX_BUF_SIZE, executor::ffi::ExecutorFfi as _};

/// Issue 2014 — byte of a get_result reply where the result fields start
/// (`[encap 4][status i8][pad → 8][fields]`). The C++ `PollingActionClient`
/// decodes the raw reply itself, so it needs the number; this is the C ABI
/// mirror of `nros_node::action_frame::RESULT_PAYLOAD_OFFSET` (a literal,
/// because cbindgen emits only literals — the assert below binds the two).
pub const NROS_CPP_ACTION_RESULT_PAYLOAD_OFFSET: usize = 8;
/// Issue 2014 — byte of a feedback message where the feedback fields start
/// (`[encap 4][GoalId.uuid 16][fields]`); mirror of
/// `nros_node::action_frame::FEEDBACK_PAYLOAD_OFFSET`.
pub const NROS_CPP_ACTION_FEEDBACK_PAYLOAD_OFFSET: usize = 20;
/// Issue 2014 — length of the CDR encapsulation header a raw action frame
/// starts with; mirror of `nros_node::action_frame::CDR_HEADER_LEN`.
pub const NROS_CPP_ACTION_CDR_HEADER_LEN: usize = 4;
const _: () = assert!(NROS_CPP_ACTION_RESULT_PAYLOAD_OFFSET == action_frame::RESULT_PAYLOAD_OFFSET);
const _: () =
    assert!(NROS_CPP_ACTION_FEEDBACK_PAYLOAD_OFFSET == action_frame::FEEDBACK_PAYLOAD_OFFSET);
const _: () = assert!(NROS_CPP_ACTION_CDR_HEADER_LEN == action_frame::CDR_HEADER_LEN);

use crate::{
    CppContext, DispatchGuard, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_NOT_ALLOWED, NROS_CPP_RET_NOT_FOUND, NROS_CPP_RET_NOT_INIT, NROS_CPP_RET_OK,
    NROS_CPP_RET_REENTRANT, NROS_CPP_RET_REJECTED, NROS_CPP_RET_TIMEOUT,
    NROS_CPP_RET_TRANSPORT_ERROR, NROS_CPP_RET_TRY_AGAIN, NROS_CPP_RET_UNSUPPORTED,
    cpp_ctx_checked, cstr_to_str, nros_cpp_node_t, nros_cpp_qos_t, nros_cpp_ret_t,
};

/// Scratch buffer for re-framing an incoming goal payload before handing
/// it to the user goal callback: strip the action framing
/// (`[CDR_HDR][seq_prefix][UUID]`) and re-prepend a plain CDR header so the
/// callback sees a normal `[CDR_HDR][goal_fields]` message.
const GOAL_USER_BUF: usize = 512;

// Phase 87.11: opaque storage sizes are now driven by layout-mirror
// structs in `nros::sizes` (see `CppActionServerLayout` /
// `CppActionClientLayout`). The asserts below ensure the real
// `CppActionServer` / `CppActionClient` layouts stay byte-equivalent
// to the mirrors — any field-shape change here must be paired with an
// update in `packages/api/nros/src/sizes.rs`.

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

/// Accepted-goal callback type invoked by the arena's post-accept hook.
///
/// Issue 0796 — the C++ callback tier had no way to register one, so a user
/// who returned `AcceptAndDefer` from the goal callback was never told the
/// goal had been accepted: the exact point at which a deferred goal is
/// supposed to start executing. C takes `rclc_accepted_callback_t` at
/// `rclc_action_server_init_with_callbacks` and Rust takes one in
/// `create_action_server_with_callbacks(goal, cancel, accepted)`; this is the
/// C++ spelling of the same hook.
///
/// Fires AFTER the accept reply has reached the client (see
/// [`nros_node::RawAcceptedCallback`]), for both `AcceptAndExecute` and
/// `AcceptAndDefer`, so a long-running body here does not delay the accept.
pub type CppAcceptedCallback = unsafe extern "C" fn(goal_id: *const [u8; 16], ctx: *mut c_void);

/// Internal state for the action server.
///
/// Holds the arena handle and the user-registered callbacks. Phase 87.6
/// thin-wrapper refactor: the `action_name` / `type_name` / `type_hash`
/// buffers moved to the C++ `rclcpp_action::Server<A>` class (passed to
/// `nros_cpp_action_server_register` at registration time). No C++-side
/// goal queue — the arena in `nros-node` owns all lifecycle state.
///
/// `#[repr(C)]` is load-bearing: the layout-mirror assert below compares this
/// against the `#[repr(C)]` `nros::sizes::CppActionServerLayout`. Without it
/// the compiler may reorder/repack fields — the sizes coincide on 64-bit hosts
/// but diverge on 32-bit targets (e.g. armv7a-nuttx-eabihf), tripping the
/// assert. repr(C) pins both to identical declaration-order layout everywhere.
#[repr(C)]
pub(crate) struct CppActionServer {
    handle: Option<nros_node::ActionServerRawHandle>,
    goal_cb: Option<CppGoalCallback>,
    cancel_cb: Option<CppCancelCallback>,
    /// issue 0796 — post-accept hook, `None` until
    /// `nros_cpp_action_server_set_accepted_callback` installs one.
    accepted_cb: Option<CppAcceptedCallback>,
    cb_ctx: *mut c_void,
    /// Phase 104.C.9.b — NodeId captured at create time, consumed by
    /// `nros_cpp_action_server_register` to pick the `_on(NodeId, ...)`
    /// multi-Node dispatch variant. Issue 0312 — BIASED BY ONE, mirroring
    /// `nros_cpp_node_t::node_id`: `0` = no node, `n` = `NodeId(n - 1)`.
    node_id: u8,
    _reserved: [u8; 7],
    /// Phase 193.4b — create-time QoS, applied to the three underlying
    /// service servers (send_goal / cancel_goal / get_result) at register
    /// time. Defaults to the services profile via `nros_cpp_qos_t`.
    qos: nros_cpp_qos_t,
}

// Layout-mirror equivalence (Phase 87.11): the real `CppActionServer`
// must have the same byte size as `nros::sizes::CppActionServerLayout`,
// which is what `NROS_CPP_ACTION_SERVER_SIZE` is sized to. Any field
// shape change here requires a paired update in
// `packages/api/nros/src/sizes.rs::CppActionServerLayout`.
const _: () = assert!(
    core::mem::size_of::<CppActionServer>()
        == core::mem::size_of::<nros::sizes::CppActionServerLayout>(),
    "CppActionServer size diverges from nros::sizes::CppActionServerLayout — \
     update the layout mirror to track any field-shape change"
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

    // Incoming framing: [CDR_HDR][UUID(16)][goal_fields].
    // User-facing framing: [CDR_HDR][goal_fields] (issue 2014: offset and
    // splice from `action_frame`).
    let slice = unsafe { core::slice::from_raw_parts(goal_data, goal_len) };
    let mut user_buf = [0u8; GOAL_USER_BUF];
    let (ptr, len) = match action_frame::splice_goal_request(slice, &mut user_buf) {
        Some(n) => (user_buf.as_ptr(), n),
        None => (goal_data, goal_len),
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
        return nros::CancelResponse::Accept;
    };
    let uuid_ptr = goal_id as *const [u8; 16];
    // issue 0796 — `nros::CancelResponse` is now the PER-GOAL decision, with
    // the same 0/1 discriminants as C++'s `rclcpp_action::CancelResponse` enum class,
    // so this is a straight translation rather than a hop through the
    // `CancelGoal` RPC return code.
    match unsafe { cb(uuid_ptr, server.cb_ctx) } {
        0 => nros::CancelResponse::Reject,
        _ => nros::CancelResponse::Accept,
    }
}

/// Post-accept trampoline — forwards to the user's registered accepted
/// callback, if any.
///
/// Registered unconditionally at `register_action_server_raw` time (the arena
/// captures the hook when the entry is built, before the user has had a chance
/// to call `set_accepted_callback`), so the `None` case is the normal one and
/// must be free.
///
/// # Safety
/// `context` must point to a valid `CppActionServer`.
unsafe extern "C" fn accepted_callback_trampoline(
    goal_id: *const nros::GoalId,
    context: *mut c_void,
) {
    let server = unsafe { &*(context as *const CppActionServer) };
    let Some(cb) = server.accepted_cb else {
        return;
    };
    unsafe { cb(goal_id as *const [u8; 16], server.cb_ctx) };
}

/// Create an action server on a node.
///
/// Phase 87.6: this call only zero-initialises the storage. Names and
/// executor registration happen in `nros_cpp_action_server_register`
/// below, same as before — the split exists to avoid a FreeRTOS QEMU
/// deadlock where eagerly declaring the five underlying entities blocks
/// the session mutex.
///
/// # Safety
/// `storage` must point to an 8-byte-aligned buffer of at least
/// `NROS_CPP_ACTION_SERVER_STORAGE_SIZE` bytes.
/// Issue 1913 — the hash each of an action's five endpoints is keyed on.
///
/// The C++ twin of nros-c's `rclc_action_endpoint_hashes_t` (same layout, and
/// converted through it so the NULL-fallback rule has one spelling). An Iron+
/// peer keys every endpoint on its own type, so the single `type_hash` an action
/// carried matched none of them. A NULL pointer, or a NULL member, falls back
/// to `type_hash` — what an action type generated by an older `nros` gets
/// (`nros::detail::action_endpoint_hashes<A>()`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_action_hashes_t {
    /// `<Action>_SendGoal` service hash.
    pub send_goal: *const c_char,
    /// `action_msgs/srv/CancelGoal` service hash.
    pub cancel_goal: *const c_char,
    /// `<Action>_GetResult` service hash.
    pub get_result: *const c_char,
    /// `<Action>_FeedbackMessage` hash.
    pub feedback: *const c_char,
    /// `action_msgs/msg/GoalStatusArray` hash.
    pub status: *const c_char,
}

impl nros_cpp_action_hashes_t {
    /// # Safety
    /// `p` is NULL or points at a valid struct whose non-NULL members are
    /// NUL-terminated strings outliving `'a`.
    unsafe fn resolve<'a>(
        p: *const Self,
        fallback: &'a str,
    ) -> nros_node::ActionEndpointHashes<'a> {
        let c = if p.is_null() {
            nros_c::rclc_action_endpoint_hashes_t::NULL
        } else {
            let h = unsafe { &*p };
            nros_c::rclc_action_endpoint_hashes_t {
                send_goal: h.send_goal,
                cancel_goal: h.cancel_goal,
                get_result: h.get_result,
                feedback: h.feedback,
                status: h.status,
            }
        };
        unsafe { c.resolve(fallback) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_create(
    node: *const nros_cpp_node_t,
    _action_name: *const c_char,
    _type_name: *const c_char,
    _type_hash: *const c_char,
    qos: nros_cpp_qos_t,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node.is_null() || storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let node_ref = unsafe { &*node };
    if node_ref.executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = CppActionServer {
        handle: None,
        goal_cb: None,
        cancel_cb: None,
        accepted_cb: None,
        cb_ctx: core::ptr::null_mut(),
        // Phase 104.C.9.b — capture the Node's id so register can
        // pick the multi-Node `_on(NodeId, ...)` dispatch variant.
        node_id: node_ref.node_id,
        _reserved: [0u8; 7],
        // Phase 193.4b — stash the create-time QoS for the three underlying
        // service servers (applied at register time).
        qos,
    };
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
    action_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    hashes: *const nros_cpp_action_hashes_t,
    sched_context: u8,
) -> nros_cpp_ret_t {
    if storage.is_null() || action_name.is_null() || type_name.is_null() || type_hash.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &mut *(storage as *mut CppActionServer) };
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };

    let act_str = match unsafe { cstr_to_str(action_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 1913 — each endpoint on its own hash (NULL falls back to `hash_str`).
    let hashes = unsafe { nros_cpp_action_hashes_t::resolve(hashes, hash_str) };

    // Phase 305 W3 (issue 0255) — resolve `~`/relative names + launch remaps.
    // No node handle crosses this seam (only the executor), so the executor's
    // CURRENT identity scopes the lookup.
    let resolved_act = match ctx.executor.resolve_entity_name(act_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let act_str = resolved_act.as_str();

    // Phase 104.C.9.b — `spec.node_id` routes registration through the
    // named multi-RMW Node's session (so the underlying
    // queryables/publishers land there); `None` uses the default Node.
    // Phase 193.4b — `spec.qos` applies the create-time QoS to the three
    // underlying service servers (rclcpp `create_action_server(name, qos)`).
    let qos = server.qos.to_qos_settings();
    // Issue 0312 — `CppActionServer` mirrors the handle's biased id, so decode
    // it the same way (a raw `!= 0` here would drop a single-node entry's node).
    let node_id =
        (server.node_id != 0).then(|| nros_node::executor::NodeId::from_raw(server.node_id - 1));
    let result = ctx
        .executor
        .register_action_server_raw(nros_node::RawActionServerSpec {
            node_id,
            action_name: act_str,
            type_name: type_str,
            type_hash: hash_str,
            hashes,
            qos,
            goal_callback: goal_callback_trampoline,
            cancel_callback: cancel_callback_trampoline,
            // issue 0796 — always registered; the trampoline is a no-op until
            // the user installs a callback. The arena captures this pointer
            // when the entry is built, so it cannot be added later.
            accepted_callback: Some(accepted_callback_trampoline),
            context: storage,
        });
    match result {
        Ok(handle) => {
            // Phase 189.M3.3.c — bind the action's goal-service handle to the
            // requested sched context. The C++ action server is arena-registered
            // (unlike poll-style C++ services), so its handle is real + its
            // goal/cancel callbacks are executor-dispatched — the bind is
            // functional. `0` = inherit (no-op); an unknown slot fails.
            if sched_context != 0 {
                let sc_id = nros_node::executor::sched_context::SchedContextId(sched_context);
                if ctx
                    .executor
                    .bind_handle_to_sched_context(handle.handle_id(), sc_id)
                    .is_err()
                {
                    return NROS_CPP_RET_INVALID_ARGUMENT;
                }
            }
            server.handle = Some(handle);
            NROS_CPP_RET_OK
        }
        // issue 0557 / phase-358 W5 — was `Err(_) => TRANSPORT_ERROR`, which
        // discarded the `NodeError` and reported `-100` for causes that are not
        // transport at all. That is the collapse issue 0436 already fixed for
        // `nros_cpp_init` (via this same mapper, which also names the variant on
        // the error path) — applied there and not here.
        //
        // It cost exactly what that kind of collapse costs: the Zephyr Cyclone C
        // action image failed with `run_components failed rc=-100` and no way to
        // tell which precondition the backend rejected, while the Rust action,
        // the C service and the C pubsub cells on the same backend all passed.
        Err(e) => crate::node_error_to_cpp_ret(e),
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

/// Register the accepted-goal callback on the action server (issue 0796).
///
/// Separate from `nros_cpp_action_server_set_callbacks` so that adding the
/// third callback did not change an existing FFI signature; `ctx` is the SAME
/// shared context slot those two use, and passing a different pointer here
/// re-points all three. The C++ `ActionServer<A>` passes `this` from
/// `install_callbacks()` in both calls.
///
/// The callback fires once per accepted goal, after the accept reply has been
/// sent — for `AcceptAndExecute` and `AcceptAndDefer` alike. Pass a null
/// `accepted_cb` to clear it.
///
/// # Safety
/// `handle` must be a valid initialized action server storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_set_accepted_callback(
    handle: *mut c_void,
    accepted_cb: Option<CppAcceptedCallback>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let server = unsafe { &mut *(handle as *mut CppActionServer) };
    server.accepted_cb = accepted_cb;
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
    if handle.is_null() || goal_id.is_null() || feedback_buf.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &*(handle as *const CppActionServer) };
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };
    let data = unsafe { core::slice::from_raw_parts(feedback_buf, feedback_len) };
    // Phase 177.9.F — strip the C++ serializer's CDR header before handing
    // feedback to the core, exactly as `nros_cpp_action_server_complete_goal`
    // does for results. The core action storage keeps FIELDS ONLY; the
    // client-side `cpp_feedback_trampoline` re-adds a CDR header before C++
    // deserialize. Without this strip the feedback travelled with its header,
    // the trampoline added a second one, and the C++ deserializer read the
    // inner header's bytes as the sequence length → garbage → `feedback=0`
    // on the cpp/xrce action E2E.
    let feedback_fields = strip_cdr_header(data);

    let h = match &server.handle {
        Some(h) => h,
        None => return NROS_CPP_RET_ERROR,
    };
    match h.publish_feedback_raw(ctx.executor, &id, feedback_fields) {
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
    status: i32,
    result_buf: *const u8,
    result_len: usize,
) -> nros_cpp_ret_t {
    if handle.is_null() || goal_id.is_null() || result_buf.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let server = unsafe { &*(handle as *const CppActionServer) };
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };
    let data = unsafe { core::slice::from_raw_parts(result_buf, result_len) };
    // Core action storage keeps result fields only; C++ serializers include
    // the CDR header for user-facing deserialize calls.
    let result_fields = strip_cdr_header(data);

    let h = match &server.handle {
        Some(h) => h,
        None => return NROS_CPP_RET_ERROR,
    };
    // issue 0796 — this used to pass `Succeeded` unconditionally, so a C++
    // callback-tier server that aborted a goal reported it to the client as
    // succeeded. Every other surface (C, the C++ polling tier, both Rust
    // servers) takes a status; this one now does too.
    let Some(status) = nros::GoalStatus::from_i8(status as i8) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    // issue 0796 — a result the server cannot retain (larger than the action
    // server's RESULT_BUF) is now reported instead of silently dropped.
    match h.complete_goal_raw(ctx.executor, &id, status, result_fields) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Start EXECUTING an accepted goal — issue 2027, `ServerGoalHandle::execute`.
///
/// Upstream's `execute()` moves a goal from ACCEPTED to EXECUTING and
/// publishes the status array; it is how a goal the goal callback answered
/// `ACCEPT_AND_DEFER` starts (and what the server does itself, before
/// `handle_accepted`, for `ACCEPT_AND_EXECUTE`). Returns `NROS_CPP_RET_OK`
/// when the goal moved, `NROS_CPP_RET_NOT_FOUND` when this server has no such
/// live goal, and `NROS_CPP_RET_NOT_ALLOWED` when the goal is not ACCEPTED
/// (upstream raises there; nothing is changed or published).
///
/// # Safety
/// `handle` must be a valid `CppActionServer` storage pointer,
/// `executor_handle` a valid `CppContext`, and `goal_id` 16 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_execute(
    handle: *mut c_void,
    executor_handle: *mut c_void,
    goal_id: *const [u8; 16],
) -> nros_cpp_ret_t {
    if handle.is_null() || goal_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let server = unsafe { &*(handle as *const CppActionServer) };
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let Some(h) = server.handle else {
        return NROS_CPP_RET_NOT_INIT;
    };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };
    let mut status = None;
    h.for_each_active_goal(&*ctx.executor, |g| {
        if g.goal_id.uuid == id.uuid {
            status = Some(g.status);
        }
    });
    match status {
        None => NROS_CPP_RET_NOT_FOUND,
        Some(nros::GoalStatus::Accepted) => {
            h.set_goal_status(ctx.executor, &id, nros::GoalStatus::Executing);
            NROS_CPP_RET_OK
        }
        Some(_) => NROS_CPP_RET_NOT_ALLOWED,
    }
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
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(visitor) = visitor else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let server = unsafe { &*(handle as *const CppActionServer) };
    let Some(executor_ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let executor_ctx = &*executor_ctx;
    let arena_handle = match &server.handle {
        Some(h) => *h,
        None => return NROS_CPP_RET_ERROR,
    };
    arena_handle.for_each_active_goal(executor_ctx.executor, |g| unsafe {
        let uuid_ptr: *const [u8; 16] = &g.goal_id.uuid;
        visitor(uuid_ptr, g.status as i8, ctx);
    });
    NROS_CPP_RET_OK
}

/// Is `goal_id` a goal this server still knows about? — phase-417 W4.b.
///
/// The C++ tier's binding of `rclc_action_server_goal_exists` /
/// `ActionServerRawHandle::goal_exists`. TRUE while the goal is active AND
/// while its completed result is still retained, which is rcl's window.
///
/// Returns `false` rather than an error code for every rejected argument: the
/// ported idiom is a guard, and a `Result` here would make the one-line `if`
/// that a caller writes impossible to write.
///
/// # Safety
/// `handle` must be a valid `CppActionServer` storage pointer,
/// `executor_handle` a valid `CppContext`, and `goal_id` 16 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_goal_exists(
    handle: *const c_void,
    executor_handle: *mut c_void,
    goal_id: *const [u8; 16],
) -> bool {
    if handle.is_null() || goal_id.is_null() {
        return false;
    }
    let server = unsafe { &*(handle as *const CppActionServer) };
    let Some(executor_ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return false;
    };
    let executor_ctx = &*executor_ctx;
    let arena_handle = match &server.handle {
        Some(h) => *h,
        None => return false,
    };
    let goal = nros_node::GoalId {
        uuid: unsafe { *goal_id },
    };
    arena_handle.goal_exists(executor_ctx.executor, &goal)
}

/// RELEASE the arena entry this storage registered — issue 1496.
///
/// (Named `detach` because it first landed as one; since resolution 2 it ends
/// the entry outright. The symbol is kept: it is cbindgen output reached from
/// `~Server()` and the move-assignment, and the stronger behaviour is what
/// both callers wanted.)
///
/// Call this BEFORE [`nros_cpp_action_server_destroy`], from the destructor or
/// move-assignment of whatever owns `storage`. The two are separate calls for
/// the reason issue 0796 gives one line up: `nros_cpp_action_server_destroy`
/// takes one argument and is declared in three places, so growing it to take
/// the executor would have broken every caller. `CppActionServer` does not keep
/// an executor pointer of its own (the client does — see
/// [`nros_cpp_action_client_destroy`]), and nothing in this crate can recover
/// one from a bare storage pointer: `CppContext` lives in caller-provided
/// storage and there is no registry of them.
///
/// What it releases and why: `nros_cpp_action_server_register` passes
/// `storage` itself as the arena entry's callback `context`, and the goal /
/// cancel / accepted trampolines all read `CppActionServer` back out of it, so
/// an entry that outlived its owner dispatched into freed storage. The entry is
/// now DROPPED in place (`Executor::release_action_server_raw`): its three
/// service servers and two publishers are destroyed, so no late `send_goal`
/// can reach it; its callback slot is freed; and its arena bytes go on the
/// executor's free list, where the next registration that fits reuses them —
/// a create/destroy loop no longer exhausts `NROS_EXECUTOR_MAX_CBS` and then
/// the arena.
///
/// Returns `NROS_CPP_RET_OK` also when there is nothing to detach (an
/// unregistered server, or an executor handle that is not ours) — the caller is
/// on a destruction path and has no way to act on a failure.
///
/// # Safety
/// `storage` must be a valid initialized action server storage, or NULL
/// (no-op). `executor_handle` must be the executor this server was registered
/// on, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_detach(
    storage: *mut c_void,
    executor_handle: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    let server = unsafe { &mut *(storage as *mut CppActionServer) };
    let Some(handle) = server.handle.take() else {
        // Created but never registered: no arena entry names this storage.
        return NROS_CPP_RET_OK;
    };
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_OK;
    };
    // SAFETY: `handle` came from `register_action_server_raw` in
    // `nros_cpp_action_server_register`, the DEFAULT-sized entry point, which
    // is what the release's contract asks for; it was `take`n above, so no
    // copy of it survives in this storage to be used afterwards.
    unsafe { ctx.executor.release_action_server_raw(&handle) };
    server.goal_cb = None;
    server.cancel_cb = None;
    server.accepted_cb = None;
    server.cb_ctx = core::ptr::null_mut();
    NROS_CPP_RET_OK
}

/// ABANDON an action server's local storage — issue 1496.
///
/// Named `destroy` and shaped like a release, but `CppActionServer` is a handle
/// plus three callback pointers and a QoS block: every field is `Copy` or a raw
/// pointer, `ActionServerRawHandle` is explicitly `Copy` with no `Drop`, and so
/// the `drop_in_place` below **runs no destructor**. That is deliberate: the
/// five RMW entities (three service servers, the feedback and status
/// publishers), the goal tables and the result slab live in an
/// `ActionServerRawArenaEntry` in the executor arena, not here, so this call
/// releases nothing by itself.
///
/// What a caller MUST do is call [`nros_cpp_action_server_detach`] first —
/// since issue 1496's resolution 2 that RELEASES the arena entry (drops it,
/// frees its callback slot, returns its arena bytes for reuse). `~Server()` in
/// `action_server.hpp` does both, in that order.
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

/// Relocate a `CppActionServer` from `old_storage` to `new_storage`.
///
/// Performs the bitwise move. The C++ `ActionServer<A>` move ctor /
/// move assignment must still call `install_callbacks()` afterwards,
/// because the callback trampolines were registered with the previous
/// `this` as their context and need to be re-registered with the new
/// `this`. This FFI only transfers the runtime state; the re-install
/// step is intentionally left to the C++ side so this function stays
/// free of C++-specific pointer semantics.
///
/// # Safety
/// See `nros_cpp_publisher_relocate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut CppActionServer);
        core::ptr::write(new_storage as *mut CppActionServer, value);
    }
    NROS_CPP_RET_OK
}

/// Re-point the executor's arena entry at a server's NEW storage after
/// [`nros_cpp_action_server_relocate`] — issue 2015.
///
/// The arena entry's callback `context` is the storage address
/// `nros_cpp_action_server_register` registered, so it must follow a move, as
/// the client's does (issue 1496, `nros_cpp_action_client_relocate`). Left
/// alone it named the moved-from storage: a server moved into an
/// `rclcpp::Owned` (what `rclcpp_action::create_server` returns) dispatched its
/// first goal through dead memory and rejected every later one. A separate
/// call rather than a parameter on `relocate`, because the executor is the C++
/// object's, not the storage's. No-op for a server never registered.
///
/// # Safety
/// `storage` is the server's live (new) storage; `executor_handle` the
/// executor it was registered on, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_retarget(
    storage: *mut c_void,
    executor_handle: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let server = unsafe { &*(storage as *const CppActionServer) };
    let Some(handle) = server.handle.as_ref() else {
        return NROS_CPP_RET_OK;
    };
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_OK;
    };
    // SAFETY: `handle` came from the default-sized `register_action_server_raw`
    // (`nros_cpp_action_server_register`); `storage` is the caller's live storage.
    unsafe { ctx.executor.retarget_action_server_raw(handle, storage) };
    NROS_CPP_RET_OK
}

// ============================================================================
// Action Client
// ============================================================================

/// Internal state for the action client.
/// C++ action client callback function pointers (freestanding C++14).
#[repr(C)]
#[derive(Clone, Copy)]
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
    /// Issue 2034 — a `CancelGoal` reply the executor's spin (or `poll`) took,
    /// with the sequence id of the request it answers and its whole CDR frame
    /// (decode with [`nros_cpp_action_cancel_reply_decode`]). Installed with
    /// [`nros_cpp_action_client_set_cancel_reply_callback`].
    pub cancel_reply: CppCancelReplyCb,
    pub context: *mut c_void,
}

/// Issue 2034 — the C++ client's cancel-reply callback:
/// `(goal_id, seq, reply, reply_len, context)`.
pub(crate) type CppCancelReplyCb =
    Option<unsafe extern "C" fn(*const [u8; 16], i64, *const u8, usize, *mut c_void)>;

// Not derived: that needs `Default` for a raw pointer, which is Rust 1.88, and
// this crate's clippy MSRV is 1.87 (`clippy.toml`). The pinned stable clippy
// honours that; the nightly one the NuttX build-std lint uses does not.
#[allow(clippy::derivable_impls)]
impl Default for CppActionClientCallbacks {
    fn default() -> Self {
        Self {
            goal_response: None,
            feedback: None,
            result: None,
            cancel_reply: None,
            context: core::ptr::null_mut(),
        }
    }
}

/// Internal state for the C++ action client.
///
/// Lightweight — the `ActionClientCore` lives in the executor's arena.
/// This struct stores the arena entry index, executor pointer, and callbacks.
/// Phase 87.6: the action_name buffer moved to the C++
/// `rclcpp_action::Client<A>` class.
///
/// `#[repr(C)]` is load-bearing (see `CppActionServer`): pins the layout to
/// match `#[repr(C)]` `nros::sizes::CppActionClientLayout` on 32-bit targets
/// too, where repr(Rust) packing would otherwise diverge.
#[repr(C)]
pub(crate) struct CppActionClient {
    callbacks: CppActionClientCallbacks,
    arena_entry_index: i32,
    executor_ptr: *mut c_void,
}

/// Get a mutable reference to an action client's core in the executor arena.
///
/// # Safety
/// `executor_ptr` must point to a valid `CppContext`.
unsafe fn cpp_arena_core_mut<'a>(
    arena_entry_index: i32,
    executor_ptr: *mut c_void,
) -> Option<crate::Locked<'a, nros_node::ActionClientCore>> {
    if arena_entry_index < 0 || executor_ptr.is_null() {
        return None;
    }
    unsafe {
        let mut ctx = crate::cpp_ctx_locked(executor_ptr);
        let core: *mut nros_node::ActionClientCore = ctx
            .executor
            .action_client_core_mut(arena_entry_index as usize)?;
        // SAFETY (phase-509 P9b): the core lives in the executor's pinned
        // arena; it is used only while `ctx` keeps the executor lock.
        Some(crate::Locked::new(core, Some(ctx)))
    }
}

// Layout-mirror equivalence (Phase 87.11). See the matching assert
// above for `CppActionServer`.
const _: () = assert!(
    core::mem::size_of::<CppActionClient>()
        == core::mem::size_of::<nros::sizes::CppActionClientLayout>(),
    "CppActionClient size diverges from nros::sizes::CppActionClientLayout — \
     update the layout mirror to track any field-shape change"
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

/// Issue 2034 — the arena's cancel-reply callback: forward the reply, with its
/// request's sequence id and whole frame, to the C++ client's own callback.
/// A client with none installed yet drops it (the C++ `Client<A>` installs one
/// at creation, before anything can be sent).
unsafe extern "C" fn cpp_cancel_response_trampoline(
    goal_id: *const nros::GoalId,
    seq: i64,
    _return_code: nros::CancelReturnCode,
    _goals_canceling: usize,
    reply: *const u8,
    reply_len: usize,
    context: *mut c_void,
) {
    let client = unsafe { &*(context as *const CppActionClient) };
    if let Some(cb) = client.callbacks.cancel_reply {
        unsafe {
            cb(
                &(*goal_id).uuid,
                seq,
                reply,
                reply_len,
                client.callbacks.context,
            )
        };
    }
}

/// Issue #179 — a raw payload never starts with a CDR encapsulation
/// identifier (`00 {00|01|06|07|0a|0b}`). The upstream dispatch (arena /
/// poll) now delivers action payloads WITH a single encap header (splicing
/// one in when a C server's stripped-fields reply arrives raw), so the
/// trampolines must prepend ONLY when the header is genuinely absent —
/// the old unconditional prepend double-framed every zenoh delivery.
fn payload_has_cdr_encap(buf: &[u8]) -> bool {
    buf.len() >= 2 && buf[0] == 0 && matches!(buf[1], 0 | 1 | 6 | 7 | 0x0a | 0x0b)
}

/// Frame a dispatched payload for a C++ callback: verbatim when it already
/// carries an encapsulation header, else behind a little-endian one.
fn frame_for_callback(data: &[u8], framed: &mut [u8; DEFAULT_RX_BUF_SIZE]) -> usize {
    if payload_has_cdr_encap(data) {
        let n = data.len().min(DEFAULT_RX_BUF_SIZE);
        framed[..n].copy_from_slice(&data[..n]);
        n
    } else {
        framed[..CDR_HEADER_LEN].copy_from_slice(&nros::cdr::CDR_LE_HEADER);
        let n = data.len().min(DEFAULT_RX_BUF_SIZE - CDR_HEADER_LEN);
        framed[CDR_HEADER_LEN..CDR_HEADER_LEN + n].copy_from_slice(&data[..n]);
        CDR_HEADER_LEN + n
    }
}

// Issue 2011 — the feedback and result trampolines FORWARD, and keep nothing.
// They used to copy every payload into `FEEDBACK_STASH` / `RESULT_STASH`, two
// `static mut` single slots shared by every C++ action client in the process,
// which `try_recv_feedback` / `try_recv_result` (and so every
// `get_result_future`) read back: two goals, or two clients, read each other's
// result and a second result overwrote an unread one. Delivery is per client
// now — `rclcpp_action::Client<A>` installs its own callbacks, which hand each
// acceptance and result to the future waiting on THAT goal (keyed by goal id,
// `rclcpp::detail::PendingList`) before forwarding to the user's.
unsafe extern "C" fn cpp_feedback_trampoline(
    goal_id: *const nros::GoalId,
    feedback_data: *const u8,
    feedback_len: usize,
    context: *mut c_void,
) {
    let client = unsafe { &*(context as *const CppActionClient) };
    let Some(cb) = client.callbacks.feedback else {
        return;
    };
    let data = unsafe { core::slice::from_raw_parts(feedback_data, feedback_len) };
    let mut framed = [0u8; DEFAULT_RX_BUF_SIZE];
    let framed_len = frame_for_callback(data, &mut framed);
    unsafe {
        cb(
            &(*goal_id).uuid,
            framed.as_ptr(),
            framed_len,
            client.callbacks.context,
        )
    };
}

unsafe extern "C" fn cpp_result_trampoline(
    goal_id: *const nros::GoalId,
    status: nros::GoalStatus,
    result_data: *const u8,
    result_len: usize,
    context: *mut c_void,
) {
    let client = unsafe { &*(context as *const CppActionClient) };
    let Some(cb) = client.callbacks.result else {
        return;
    };
    // User callbacks expect normal CDR bytes. The upstream dispatch delivers
    // WITH a single encap header (issue #179); prepend only when it is
    // genuinely absent.
    let data = unsafe { core::slice::from_raw_parts(result_data, result_len) };
    let mut framed = [0u8; DEFAULT_RX_BUF_SIZE];
    let framed_len = frame_for_callback(data, &mut framed);
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
            framed.as_ptr(),
            framed_len,
            client.callbacks.context,
        )
    };
}

/// Issue 2011 — what a BLOCKING call (`nros_cpp_action_client_send_goal` /
/// `_get_result`) waits on, on its own stack frame.
///
/// Both used to install a callback that wrote a `static mut` flag, with the
/// client's callback context set to NULL: process-global (two clients blocking
/// at once shared one flag), blind to the goal (ANY acceptance or result —
/// another goal's, another future's — ended the wait as if it were this one's),
/// and it swallowed every other reply that arrived meanwhile, feedback and
/// results included, because the user callbacks were simply unplugged.
///
/// Now all three callbacks are swapped for [`waiter_goal_response`] /
/// [`waiter_feedback`] / [`waiter_result`] with THIS waiter as their context.
/// The reply for `want` ends the wait; everything else is forwarded to the
/// callbacks that were installed before (`orig`, with their own context), so a
/// pending future or the user's callback still gets its own reply. The guard
/// restores `orig` on every exit.
struct BlockingWaiter {
    want: [u8; 16],
    want_result: bool,
    orig: CppActionClientCallbacks,
    done: bool,
    accepted: bool,
    len: usize,
    buf: [u8; DEFAULT_RX_BUF_SIZE],
}

impl BlockingWaiter {
    fn new(want: [u8; 16], want_result: bool, orig: CppActionClientCallbacks) -> Self {
        Self {
            want,
            want_result,
            orig,
            done: false,
            accepted: false,
            len: 0,
            buf: [0u8; DEFAULT_RX_BUF_SIZE],
        }
    }
}

unsafe extern "C" fn waiter_goal_response(
    accepted: bool,
    goal_id: *const [u8; 16],
    ctx: *mut c_void,
) {
    let w = unsafe { &mut *(ctx as *mut BlockingWaiter) };
    if !w.want_result && !w.done && unsafe { *goal_id } == w.want {
        w.done = true;
        w.accepted = accepted;
        return;
    }
    if let Some(cb) = w.orig.goal_response {
        unsafe { cb(accepted, goal_id, w.orig.context) };
    }
}

unsafe extern "C" fn waiter_feedback(
    goal_id: *const [u8; 16],
    data: *const u8,
    len: usize,
    ctx: *mut c_void,
) {
    let w = unsafe { &mut *(ctx as *mut BlockingWaiter) };
    if let Some(cb) = w.orig.feedback {
        unsafe { cb(goal_id, data, len, w.orig.context) };
    }
}

unsafe extern "C" fn waiter_result(
    goal_id: *const [u8; 16],
    status: i32,
    data: *const u8,
    len: usize,
    ctx: *mut c_void,
) {
    let w = unsafe { &mut *(ctx as *mut BlockingWaiter) };
    if w.want_result && !w.done && unsafe { *goal_id } == w.want {
        let n = len.min(DEFAULT_RX_BUF_SIZE);
        unsafe { core::ptr::copy_nonoverlapping(data, w.buf.as_mut_ptr(), n) };
        w.len = n;
        w.done = true;
        return;
    }
    if let Some(cb) = w.orig.result {
        unsafe { cb(goal_id, status, data, len, w.orig.context) };
    }
}

/// Issue 2034 — a blocking wait wants no cancel reply: forward each one to the
/// client's own callback, so a cancel future or stash the wait's spin feeds
/// still gets it.
unsafe extern "C" fn waiter_cancel_reply(
    goal_id: *const [u8; 16],
    seq: i64,
    reply: *const u8,
    len: usize,
    ctx: *mut c_void,
) {
    let w = unsafe { &mut *(ctx as *mut BlockingWaiter) };
    if let Some(cb) = w.orig.cancel_reply {
        unsafe { cb(goal_id, seq, reply, len, w.orig.context) };
    }
}

/// Installs a [`BlockingWaiter`]'s callbacks on a client and restores the
/// previous ones when dropped.
struct WaiterInstall {
    client: *mut CppActionClient,
    orig: CppActionClientCallbacks,
}

impl WaiterInstall {
    /// # Safety
    /// `client` and `waiter` must outlive the returned guard.
    unsafe fn new(client: *mut CppActionClient, waiter: *mut BlockingWaiter) -> Self {
        let c = unsafe { &mut *client };
        let orig = c.callbacks;
        c.callbacks = CppActionClientCallbacks {
            goal_response: Some(waiter_goal_response),
            feedback: Some(waiter_feedback),
            result: Some(waiter_result),
            cancel_reply: Some(waiter_cancel_reply),
            context: waiter as *mut c_void,
        };
        Self { client, orig }
    }
}

impl Drop for WaiterInstall {
    fn drop(&mut self) {
        unsafe { (*self.client).callbacks = self.orig };
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
    hashes: *const nros_cpp_action_hashes_t,
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
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 1913 — each endpoint on its own hash (NULL falls back to `hash_str`).
    let hashes = unsafe { nros_cpp_action_hashes_t::resolve(hashes, hash_str) };

    let mut ctx_ref = unsafe { crate::cpp_ctx_locked(node_ref.executor) };
    let ctx: &mut crate::CppContext = &mut ctx_ref;

    // Phase 305 W3 (issue 0255) — resolve `~`/relative names + launch remaps.
    let resolved_act = match crate::resolve_node_entity_name(ctx, node_ref, act_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let act_str = resolved_act.as_str();

    // Register with executor — creates the ONLY ActionClientCore in the arena.
    // Trampolines read from CppActionClient.callbacks (set later via set_callbacks).
    // Phase 104.C.9.b — `spec.node_id` routes multi-Node action clients
    // through the named Node's session; `None` uses the default Node.
    let node_id = crate::node_id_opt(node_ref);
    let handle = match ctx
        .executor
        .register_action_client_raw(nros_node::RawActionClientSpec {
            node_id,
            action_name: act_str,
            type_name: type_str,
            type_hash: hash_str,
            hashes,
            goal_response_callback: Some(cpp_goal_response_trampoline),
            feedback_callback: Some(cpp_feedback_trampoline),
            result_callback: Some(cpp_result_trampoline),
            // Issue 2034 — the spin TAKES every cancel reply (which frees its
            // request's slot, `MAX_CONCURRENT_GOALS`) and hands it to the C++
            // client: to the `async_cancel_*` future / callback waiting on that
            // request, else to the stash `try_recv_cancel_response` reads. It
            // was `None`, so a cancel nobody polled for held its slot and the
            // fifth was refused.
            cancel_response_callback: Some(cpp_cancel_response_trampoline),
            context: storage, // context = CppActionClient pointer
        }) {
        Ok(h) => h,
        // issue 0870 — was `NROS_CPP_RET_TRANSPORT_ERROR`, discarding a typed
        // `NodeError`. Nothing here has touched the transport yet: this is the
        // executor's arena claim, so the variant that actually fires is
        // `ExecutorFull` (NROS_EXECUTOR_MAX_CBS) — which exists, per its own
        // doc, "so the register seam can name the knob". Reporting it as a TX
        // failure sent the diagnosis at zenoh-pico's send path instead of at a
        // capacity knob. `node_error_to_cpp_ret` already maps every variant and
        // prints it; this seam simply was not calling it. Same collapse issue
        // 0557 fixed inside the mapper, one layer out.
        Err(e) => return crate::node_error_to_cpp_ret(e),
    };

    let client = CppActionClient {
        callbacks: CppActionClientCallbacks::default(),
        arena_entry_index: handle.entry_index() as i32,
        executor_ptr: node_ref.executor,
    };

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
/// phase-338 W8 — block until the action server's send-goal queryable is
/// discoverable.
///
/// C++ sibling of `rclc_action_client_wait_for_action_server`, which the C API
/// has had since phase-124 while C++ bound neither it nor a readiness probe —
/// so the C++ action-client examples hand-rolled a 3-attempt retry around
/// `send_goal`. Mirrors `rclcpp_action::Client::wait_for_action_server`.
///
/// Spins on the `send_goal` service client's readiness, which is the
/// load-bearing entity for the first `send_goal`; the same shape as
/// `ActionClient::wait_for_action_server` in
/// `nros-node/src/executor/handles.rs` (phase-428 W13).
///
/// Returns `NROS_CPP_RET_UNSUPPORTED` at once when the backend cannot know
/// whether a server is up (XRCE) — issue 1686; it used to wait out the budget
/// and report TIMEOUT, which every C++ client read as "no server".
///
/// # Safety
/// `handle` must be a valid initialized `CppActionClient`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_wait_for_action_server(
    handle: *mut c_void,
    timeout_ms: u32,
) -> nros_cpp_ret_t {
    // Issue 1556 (c) -- an application census writes and exits at its first
    // blocking call, instead of blocking.
    crate::census_finish_application_if_armed();
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let executor_ptr = client.executor_ptr;
    if executor_ptr.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    const SPIN_MS: u64 = 10;
    let deadline_ns = crate::nros_cpp_time_ns() + (timeout_ms as u64) * 1_000_000;

    // phase-428 W13 — spin, then ask again, re-borrowing the arena each time
    // so the executor borrow does not overlap.
    loop {
        {
            let Some(core) =
                (unsafe { cpp_arena_core_mut(client.arena_entry_index, executor_ptr) })
            else {
                return NROS_CPP_RET_INVALID_ARGUMENT;
            };
            // issue 1686 — one classification for every wait loop.
            match nros_node::executor::ServerVisibility::of(core.server_readiness()) {
                nros_node::executor::ServerVisibility::Visible => return NROS_CPP_RET_OK,
                nros_node::executor::ServerVisibility::Unknowable => {
                    return NROS_CPP_RET_UNSUPPORTED;
                }
                nros_node::executor::ServerVisibility::NotYet => {}
            }
        }
        if crate::nros_cpp_time_ns() >= deadline_ns {
            return NROS_CPP_RET_TIMEOUT;
        }
        let mut ctx_ref = unsafe { crate::cpp_ctx_locked(executor_ptr) };
        let ctx: &mut crate::CppContext = &mut ctx_ref;
        let _ = ctx
            .executor
            .spin_once(core::time::Duration::from_millis(SPIN_MS));
    }
}

/// Non-blocking snapshot of action-server visibility — phase-417 W4.b.
///
/// `rclcpp_action::ClientBase::action_server_is_ready`, the non-blocking half
/// of [`nros_cpp_action_client_wait_for_action_server`] and the C++ binding of
/// C's `rclc_action_client_action_server_is_ready` / Rust's
/// `ActionClient::action_server_is_ready` (ledger row
/// `cpp:Client::action_server_is_ready`, which was a C++-only gap: the other
/// two languages had the predicate and C++ could only BLOCK).
///
/// Reads the same `is_server_ready()` the wait loop polls, and spins nothing —
/// so unlike the wait it is legal inside a callback.
///
/// # Safety
/// `handle` must be a valid initialized `CppActionClient`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_action_server_is_ready(
    handle: *mut c_void,
) -> bool {
    if handle.is_null() {
        return false;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let executor_ptr = client.executor_ptr;
    if executor_ptr.is_null() {
        return false;
    }
    match unsafe { cpp_arena_core_mut(client.arena_entry_index, executor_ptr) } {
        Some(core) => core.is_server_ready(),
        None => false,
    }
}

/// Send a goal and block until the server accepts or rejects it.
///
/// # Returns
///
/// Three outcomes, three codes — they were two before issue 0868, and callers
/// could not tell a server decision from a failed send:
///
/// * `NROS_CPP_RET_OK` — the server ACCEPTED; `goal_id_out` is filled.
/// * `NROS_CPP_RET_REJECTED` — the server RECEIVED the goal and declined it.
///   This is a decision, not a fault: the server's goal callback said no.
/// * `NROS_CPP_RET_TIMEOUT` — no goal response within 30 s. The server may
///   never have received the goal at all; nothing here can tell.
/// * `NROS_CPP_RET_ERROR` — the goal could not be SENT (no arena entry, or
///   the backend refused the request).
/// * `NROS_CPP_RET_REENTRANT` — called from inside a callback. This spins the
///   executor, so it cannot run under one.
///
/// # Safety
/// All pointers must be valid; `handle` must be an initialized `CppActionClient`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_send_goal(
    handle: *mut c_void,
    goal_buf: *const u8,
    goal_len: usize,
    goal_id_out: *mut [u8; 16],
) -> nros_cpp_ret_t {
    // Issue 1556 (c) -- an application census writes and exits at its first
    // blocking call, instead of blocking.
    crate::census_finish_application_if_armed();
    if handle.is_null() || goal_buf.is_null() || goal_id_out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let goal_data = unsafe { core::slice::from_raw_parts(goal_buf, goal_len) };

    // C++ ffi_serialize produces [CDR_HEADER][fields], but send_goal_blocking
    // expects raw fields only (it adds its own CDR header + GoalId).
    let goal_fields = strip_cdr_header(goal_data);

    // Send goal via arena core (non-blocking)
    let goal_id = {
        let mut core =
            match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
                Some(c) => c,
                None => return NROS_CPP_RET_ERROR,
            };
        match core.send_goal_raw(goal_fields) {
            Ok(id) => id,
            Err(_) => return NROS_CPP_RET_ERROR,
        }
    };
    unsafe {
        *goal_id_out = goal_id.uuid;
    }

    // Spin executor until THIS goal's acceptance arrives or timeout (30 s
    // wall-clock). Issue 2011 — the wait is a `BlockingWaiter` on this frame,
    // matched by goal id; every other reply is forwarded to the callbacks that
    // were installed before (see `BlockingWaiter`).
    //
    // Phase 89.3: wall-clock budgeting instead of `for _ in 0..1000`
    // (same class of bug fixed for service clients in 89.2). On
    // multi-threaded zpico backends (POSIX/Zephyr), `spin_once(10)`
    // returns early on any incoming frame (keep-alives, discovery
    // gossip, interest messages) — a 1000-iteration budget collapses
    // to milliseconds of real time and the goal-response callback
    // never has a chance to fire before we return Error.
    //
    // Phase 160.C.2 — bumped 10 s → 30 s. Zephyr zenoh-pico's
    // declare-cascade slowness extends to the get-query path too;
    // server's queryable callback may not run for ~12 s after the
    // client sends the goal (lease/keepalive serialization). The
    // 10 s budget was racing this on test_zephyr_cpp_action_server_to_client_e2e.
    let mut ctx_ref = unsafe { crate::cpp_ctx_locked(client.executor_ptr) };
    let ctx: &mut crate::CppContext = &mut ctx_ref;
    // Issue 0290 — this helper BLOCKS by spinning the executor, so it must not
    // run from inside a callback (the C twin `rclc_action_send_goal` returns
    // RCLC_RET_REENTRANT for exactly this).
    let CppContext {
        executor,
        c: nros_c::rclc_executor_t { in_dispatch, .. },
        ..
    } = ctx;
    let Some(_guard) = DispatchGuard::enter(in_dispatch) else {
        return NROS_CPP_RET_REENTRANT;
    };
    let mut waiter = BlockingWaiter::new(goal_id.uuid, false, client.callbacks);
    let waiter_ptr: *mut BlockingWaiter = &mut waiter;
    // SAFETY: `waiter` and the client outlive `_install` (dropped first).
    let _install = unsafe { WaiterInstall::new(client, waiter_ptr) };
    let start_ns = crate::nros_cpp_time_ns();
    let timeout_ns: u64 = 30_000_000_000; // 30 s
    loop {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));
        // SAFETY: written only by the waiter callbacks, inside `spin_once`.
        let (done, accepted) = unsafe { ((*waiter_ptr).done, (*waiter_ptr).accepted) };
        if done {
            // issue 0868 — a server REJECTION is `REJECTED`, not `ERROR`.
            //
            // This used to return `ERROR` (-1), which is also what the two
            // failure paths above return when the goal could not be SENT at
            // all (no arena core, `send_goal_raw` failed). One code for
            // "the server considered this goal and said no" and "the goal
            // never left" makes the distinction unrecoverable by the caller,
            // so every C++ action-client example printed "Goal was rejected
            // by server" for all three outcomes including `TIMEOUT`.
            //
            // With this split the three are distinguishable: `REJECTED` = the
            // server decided, `ERROR` = we could not send, `TIMEOUT` = no
            // answer within the budget.
            return if accepted {
                NROS_CPP_RET_OK
            } else {
                NROS_CPP_RET_REJECTED
            };
        }
        let elapsed_ns = crate::nros_cpp_time_ns().saturating_sub(start_ns);
        if elapsed_ns >= timeout_ns {
            break;
        }
    }
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
pub unsafe extern "C" fn nros_cpp_action_client_get_result(
    handle: *mut c_void,
    executor_handle: *mut c_void,
    goal_id: *const [u8; 16],
    result_buf: *mut u8,
    result_buf_len: usize,
    result_len: *mut usize,
) -> nros_cpp_ret_t {
    // Issue 1556 (c) -- an application census writes and exits at its first
    // blocking call, instead of blocking.
    crate::census_finish_application_if_armed();
    if handle.is_null() || goal_id.is_null() || result_buf.is_null() || result_len.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let Some(_ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };

    // Send get_result request via arena core (non-blocking)
    {
        let mut core =
            match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
                Some(c) => c,
                None => return NROS_CPP_RET_ERROR,
            };
        if core.send_get_result_request(&id).is_err() {
            return NROS_CPP_RET_ERROR;
        }
    }

    // Spin executor until THIS goal's result arrives or timeout (10 s
    // wall-clock). Issue 2011 — a `BlockingWaiter` on this frame, matched by
    // goal id, instead of three `static mut`s shared by every client.
    //
    // Phase 89.3: wall-clock budgeting — see the explanation on
    // `send_goal` above for why `for _ in 0..1000` is insufficient on
    // multi-threaded zpico backends.
    let mut ctx_ref = unsafe { crate::cpp_ctx_locked(client.executor_ptr) };
    let ctx: &mut crate::CppContext = &mut ctx_ref;
    // Issue 0290 — blocking helper; refuse to re-enter from a callback.
    let CppContext {
        executor,
        c: nros_c::rclc_executor_t { in_dispatch, .. },
        ..
    } = ctx;
    let Some(_guard) = DispatchGuard::enter(in_dispatch) else {
        return NROS_CPP_RET_REENTRANT;
    };
    let mut waiter = BlockingWaiter::new(id.uuid, true, client.callbacks);
    let waiter_ptr: *mut BlockingWaiter = &mut waiter;
    // SAFETY: `waiter` and the client outlive `_install` (dropped first).
    let _install = unsafe { WaiterInstall::new(client, waiter_ptr) };
    let start_ns = crate::nros_cpp_time_ns();
    let timeout_ns: u64 = 10_000_000_000; // 10 s
    loop {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));
        // SAFETY: written only by the waiter callbacks, inside `spin_once`.
        let w = unsafe { &*waiter_ptr };
        if w.done {
            let data_len = w.len;
            if data_len <= result_buf_len {
                unsafe {
                    core::ptr::copy_nonoverlapping(w.buf.as_ptr(), result_buf, data_len);
                    *result_len = data_len;
                }
                return NROS_CPP_RET_OK;
            }
            return NROS_CPP_RET_ERROR;
        }
        let elapsed_ns = crate::nros_cpp_time_ns().saturating_sub(start_ns);
        if elapsed_ns >= timeout_ns {
            break;
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

    // Issue 2011 — this reads only feedback the executor has NOT dispatched:
    // during `spin_once` the arena takes it off the wire and hands it to the
    // client's feedback callback. The process-global `FEEDBACK_STASH` that used
    // to catch those (for every client at once) is gone;
    // `rclcpp_action::Client<A>` keeps its own latest-feedback slot, filled by
    // the callback it installs.
    let client = unsafe { &mut *(handle as *mut CppActionClient) };

    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
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
            // Issue 2014 — offset and splice from `action_frame` (the frame's
            // own encap in front of the fields, as every dispatch delivers).
            let msg = &core.feedback_buffer_ref()[..total_len];
            if let Some(fields) = action_frame::feedback_fields(msg)
                && CDR_HEADER_LEN + fields.len() <= buf_len
            {
                unsafe {
                    let out = core::slice::from_raw_parts_mut(feedback_buf, buf_len);
                    *feedback_len = action_frame::splice(msg, fields, out).unwrap_or(0);
                }
                return NROS_CPP_RET_OK;
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
    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => {
                unsafe { *out_len = 0 };
                return NROS_CPP_RET_TRY_AGAIN;
            }
        };

    match core.try_recv_send_goal_reply_for() {
        Ok(Some((total_len, goal_id))) => {
            // The reply buffer contains: CDR header (4) + accepted byte (1) + ...
            // We produce: goal_id (16) + accepted (1) = 17 bytes
            let needed = 17usize;
            if needed > out_capacity {
                unsafe { *out_len = needed };
                return NROS_CPP_RET_ERROR;
            }
            let buf = core.result_buffer_ref();
            let accepted = u8::from(action_frame::goal_accepted(&buf[..total_len]));
            // Issue 2009 — the goal the reply's request named. This used to be
            // rebuilt from `goal_counter`, the most recently SENT goal.
            let uuid = goal_id.uuid;
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

/// Decode the goal-acceptance payload into split out-params.
///
/// Issue 0329 — the goal-accept wire layout (16-byte goal_id, then one
/// `accepted` byte) is produced by
/// `nros_cpp_action_client_try_recv_goal_response` above. It was ALSO decoded by
/// hand in the C++ `action_client.hpp` header (`GoalAccept::ffi_deserialize`) —
/// the same 17-byte layout in two places, a drift no ABI assert covers. This
/// owns the read Rust-side so the header only adapts types.
///
/// # Safety
/// `data` must point to at least `len` readable bytes; `out_goal_id` /
/// `out_accepted` must be valid writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_goal_accept_decode(
    data: *const u8,
    len: usize,
    out_goal_id: *mut [u8; 16],
    out_accepted: *mut bool,
) -> nros_cpp_ret_t {
    if data.is_null() || out_goal_id.is_null() || out_accepted.is_null() || len < 17 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(data, (*out_goal_id).as_mut_ptr(), 16);
        *out_accepted = *data.add(16) != 0;
    }
    NROS_CPP_RET_OK
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

    // Issue 2011 — this reads only a result the executor has NOT dispatched
    // (during `spin_once` the arena hands each result to the client's result
    // callback), and says nothing about WHICH goal it is for. The future
    // `rclcpp_action::Client<A>::get_result_future` returns no longer reads it:
    // it is keyed by goal id and fed by the callback the client installs. The
    // process-global `RESULT_STASH` this used to read first — one slot shared
    // by every C++ action client, which is how two futures read each other's
    // result — is gone.
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => {
                unsafe { *out_len = 0 };
                return NROS_CPP_RET_TRY_AGAIN;
            }
        };

    match core.try_recv_get_result_reply() {
        Ok(Some(total_len)) => {
            // Issue 2014 — the payload offset from `action_frame`.
            let buf = core.result_buffer_ref();
            let fields = match action_frame::result_fields(&buf[..total_len]) {
                Some(f) if !f.is_empty() => f,
                _ => {
                    unsafe { *out_len = 0 };
                    return NROS_CPP_RET_OK;
                }
            };
            let data_len = fields.len();
            if data_len > out_capacity {
                unsafe { *out_len = data_len };
                return NROS_CPP_RET_ERROR;
            }
            unsafe {
                core::ptr::copy_nonoverlapping(fields.as_ptr(), out_data, data_len);
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

/// RELEASE the arena entry, then ABANDON an action client's local storage —
/// issue 1496.
///
/// The `drop_in_place` at the bottom runs no destructor: `CppActionClient` is
/// three callback pointers, a context pointer, an `i32` index and an executor
/// pointer — all `Copy`. The state that matters is not here: the
/// `ActionClientCore` with its three service clients and its feedback
/// subscription lives in an `ActionClientRawArenaEntry` in the executor arena.
///
/// So this call first RELEASES that entry — without a signature change,
/// because `CppActionClient` keeps its own `executor_ptr` and
/// `arena_entry_index` (`Executor::release_action_client_raw`): it is dropped
/// in place, so its service clients and subscription are destroyed and no late
/// reply can dispatch through this storage's address; its callback slot is
/// freed; and its arena bytes go on the executor's free list for the next
/// registration that fits. A create/destroy loop therefore no longer exhausts
/// `NROS_EXECUTOR_MAX_CBS` and then the arena (issue 1496's resolution 2).
///
/// # Safety
/// `storage` must be a valid initialized action client storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    // issue 1496 — RELEASE the arena entry first: its RMW entities are
    // destroyed, its slot freed and its bytes returned for reuse.
    {
        let client = unsafe { &mut *(storage as *mut CppActionClient) };
        let entry_index = client.arena_entry_index;
        if entry_index >= 0 {
            if let Some(mut ctx) = unsafe { cpp_ctx_checked(client.executor_ptr) } {
                // SAFETY: the entry was registered by
                // `nros_cpp_action_client_create` through
                // `register_action_client_raw`, the default-sized entry point,
                // and `arena_entry_index` is cleared below so it is not reused.
                unsafe { ctx.executor.release_action_client_raw(entry_index as usize) };
            }
        }
        client.arena_entry_index = -1;
        client.callbacks = CppActionClientCallbacks::default();
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut CppActionClient);
    }
    NROS_CPP_RET_OK
}

/// Relocate a `CppActionClient` from `old_storage` to `new_storage`.
///
/// The action client's async callback context (`options.context`) is
/// user-provided, so it stays valid across the move. Relocation is a
/// straight `ptr::read` + `ptr::write`.
///
/// # Safety
/// See `nros_cpp_publisher_relocate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut CppActionClient);
        core::ptr::write(new_storage as *mut CppActionClient, value);
    }
    // issue 1496 — the arena entry's callback `context` is the storage address
    // `nros_cpp_action_client_create` registered, so it must follow the move;
    // left alone it names the moved-from storage, which a temporary frees.
    let client = unsafe { &*(new_storage as *const CppActionClient) };
    if client.arena_entry_index >= 0
        && let Some(mut ctx) = unsafe { cpp_ctx_checked(client.executor_ptr) }
    {
        // SAFETY: registered through `register_action_client_raw`, the
        // default-sized entry point; `new_storage` is the caller's live storage.
        unsafe {
            ctx.executor
                .retarget_action_client_raw(client.arena_entry_index as usize, new_storage)
        };
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

    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
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

    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => return NROS_CPP_RET_ERROR,
        };

    match core.send_get_result_request(&id) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Cancel a goal (non-blocking) — issue 0796.
///
/// Sends the `action_msgs/srv/CancelGoal` request for `goal_id` and returns.
/// The RPC's return code arrives later; drain it with
/// [`nros_cpp_action_client_try_recv_cancel_response`].
///
/// The callback tier had no cancel at all: C has `rclc_action_cancel_goal`,
/// Rust has `ActionClient::cancel_goal`, and C++ had only the L1 polling
/// tier's `PollingActionClient::send_cancel_request` — so a C++ application
/// written against the callback tier could start a goal it could not stop.
/// This is the same shape as C's: fire the request, read the outcome later.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_cancel_goal(
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
    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => return NROS_CPP_RET_ERROR,
        };
    match core.send_cancel_request(&id) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Drain the reply to a `nros_cpp_action_client_cancel_goal` (non-blocking).
///
/// Writes the `action_msgs/srv/CancelGoal` RETURN CODE (0 = Ok, 1 = Rejected,
/// 2 = UnknownGoal, 3 = GoalTerminated — `rclcpp_action::CancelReturnCode`, not the
/// per-goal `rclcpp_action::CancelResponse`; issue 0796) to `out_return_code`.
///
/// Returns `NROS_CPP_RET_OK` when a reply was consumed,
/// `NROS_CPP_RET_TRY_AGAIN` when none has arrived yet.
///
/// # Safety
/// All pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_cancel_response(
    handle: *mut c_void,
    out_return_code: *mut i8,
) -> nros_cpp_ret_t {
    if handle.is_null() || out_return_code.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => return NROS_CPP_RET_ERROR,
        };
    match core.try_recv_cancel_reply() {
        // [CDR_HDR(4)][return_code i8][goals_canceling…]
        // A reply that carries no return_code byte is a malformed frame, not
        // "no reply yet" — issue 0223's rule: do not collapse a truncation
        // onto a plausible default.
        Ok(Some(len)) => match action_frame::cancel_return_code(&core.result_buffer_ref()[..len]) {
            Some(code) => {
                unsafe { *out_return_code = code };
                NROS_CPP_RET_OK
            }
            None => NROS_CPP_RET_ERROR,
        },
        Ok(None) => NROS_CPP_RET_TRY_AGAIN,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Issue 2027 — the `send_goal` sub-client, for
/// [`nros_cpp_action_client_forget`].
pub const NROS_CPP_ACTION_CHANNEL_SEND_GOAL: i32 = 0;
/// Issue 2027 — the `cancel_goal` sub-client.
pub const NROS_CPP_ACTION_CHANNEL_CANCEL: i32 = 1;
/// Issue 2027 — the `get_result` sub-client.
pub const NROS_CPP_ACTION_CHANNEL_GET_RESULT: i32 = 2;

/// Forget ONE pending request about `goal_id` on one sub-client — issue 2027.
///
/// Each sub-client holds at most `MAX_CONCURRENT_GOALS` requests in flight,
/// and a slot used to come back only when its reply arrived. A C++ future
/// whose last owner let go first (dropped, cancelled, moved over) — with no
/// client-wide callback and no upstream goal handle still waiting for the
/// reply — left the slot taken for good, so the fifth abandoned goal refused
/// every later send. The header calls this from the future's abandonment; the
/// reply, if it still comes, is dropped by the correlator.
///
/// `channel` is one of the `NROS_CPP_ACTION_CHANNEL_*` constants. Returns `1`
/// when a request was forgotten, `0` when none about `goal_id` was pending,
/// or a negative `NROS_CPP_RET_*` code.
///
/// # Safety
/// `handle` must be a valid action client storage and `goal_id` 16 readable
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_forget(
    handle: *mut c_void,
    channel: i32,
    goal_id: *const [u8; 16],
) -> nros_cpp_ret_t {
    if handle.is_null() || goal_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };
    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => return NROS_CPP_RET_ERROR,
        };
    let forgotten = match channel {
        NROS_CPP_ACTION_CHANNEL_SEND_GOAL => core.forget_send_goal(&id),
        NROS_CPP_ACTION_CHANNEL_CANCEL => core.forget_cancel(&id),
        NROS_CPP_ACTION_CHANNEL_GET_RESULT => core.forget_get_result(&id),
        _ => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    i32::from(forgotten)
}

/// Send a `CancelGoal` request carrying a goal id AND a stamp — issue 2027.
///
/// `rclcpp_action::Client::async_cancel_all_goals` is the zero id with a zero
/// stamp, `async_cancel_goals_before(stamp)` the zero id with `stamp`. The
/// reply is correlated under `goal_id` (the zero id for both), and taken with
/// [`nros_cpp_action_client_take_cancel_reply`]. `seq_out` (may be NULL)
/// receives the request's sequence id, which the reply carries back — the
/// only thing that tells two pending cancels about one goal apart.
///
/// # Safety
/// All non-NULL pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_cancel_goals(
    handle: *mut c_void,
    goal_id: *const [u8; 16],
    stamp_sec: i32,
    stamp_nanosec: u32,
    seq_out: *mut i64,
) -> nros_cpp_ret_t {
    if handle.is_null() || goal_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let id = nros::GoalId {
        uuid: unsafe { *goal_id },
    };
    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => return NROS_CPP_RET_ERROR,
        };
    match core.send_cancel_request_stamped(&id, stamp_sec, stamp_nanosec) {
        Ok(seq) => {
            if !seq_out.is_null() {
                unsafe { *seq_out = seq };
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// One `action_msgs/msg/GoalInfo` of a `CancelGoal` reply — issue 2027.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct nros_cpp_action_goal_info_t {
    /// The goal id.
    pub uuid: [u8; 16],
    /// When the server accepted it: seconds.
    pub stamp_sec: i32,
    /// ...and nanoseconds.
    pub stamp_nanosec: u32,
}

/// Take one `CancelGoal` reply, attributed to the request it answers — issue 2027.
///
/// Writes the goal the REQUEST named (the zero id for a cancel-all /
/// cancel-before) to `goal_id_out`, that request's sequence id to `seq_out`
/// (may be NULL; `INT64_MIN` from a backend that assigns none), the RPC
/// return code to
/// `return_code_out`, the reply's `goals_canceling` count to `count_out`, and
/// the first `min(count, infos_cap)` of them to `infos`. `infos` may be NULL
/// when `infos_cap` is 0.
///
/// Returns `NROS_CPP_RET_OK` when a reply was taken, `NROS_CPP_RET_TRY_AGAIN`
/// when none is pending, and `NROS_CPP_RET_ERROR` for a malformed reply (it
/// is consumed, and `goal_id_out` / `seq_out` still name the request it
/// answered; issue 0223's rule — a truncation is not "no goals").
///
/// # Safety
/// All non-NULL pointers must be valid; `infos` must hold `infos_cap` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_take_cancel_reply(
    handle: *mut c_void,
    goal_id_out: *mut [u8; 16],
    return_code_out: *mut i8,
    infos: *mut nros_cpp_action_goal_info_t,
    infos_cap: usize,
    count_out: *mut usize,
    seq_out: *mut i64,
) -> nros_cpp_ret_t {
    if handle.is_null()
        || goal_id_out.is_null()
        || return_code_out.is_null()
        || count_out.is_null()
        || (infos.is_null() && infos_cap != 0)
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    let mut core =
        match unsafe { cpp_arena_core_mut(client.arena_entry_index, client.executor_ptr) } {
            Some(c) => c,
            None => return NROS_CPP_RET_ERROR,
        };
    let (len, seq, goal) = match core.try_recv_cancel_reply_seq() {
        Ok(Some(x)) => x,
        Ok(None) => return NROS_CPP_RET_TRY_AGAIN,
        Err(_) => return NROS_CPP_RET_ERROR,
    };
    // The request it answers is known even when the reply is malformed, so a
    // caller waiting on that request can be told it is over.
    unsafe {
        *goal_id_out = goal.uuid;
        if !seq_out.is_null() {
            *seq_out = seq;
        }
    }
    let reply = &core.result_buffer_ref()[..len];
    let (Some(code), Some(count)) = (
        action_frame::cancel_return_code(reply),
        action_frame::cancel_goals_count(reply),
    ) else {
        return NROS_CPP_RET_ERROR;
    };
    unsafe {
        *return_code_out = code;
        *count_out = count;
    }
    for i in 0..count.min(infos_cap) {
        if let Some((g, sec, nanosec)) = action_frame::cancel_goal_info(reply, i) {
            unsafe {
                *infos.add(i) = nros_cpp_action_goal_info_t {
                    uuid: g.uuid,
                    stamp_sec: sec,
                    stamp_nanosec: nanosec,
                };
            }
        }
    }
    NROS_CPP_RET_OK
}

/// Install the client's cancel-reply callback — issue 2034.
///
/// While one is installed the executor's spin (and
/// [`nros_cpp_action_client_poll`]) TAKES every `CancelGoal` reply off this
/// client and calls `cb(goal_id, seq, reply, reply_len, context)`, where
/// `goal_id` is the goal the request named (the zero id for a cancel-all /
/// cancel-before), `seq` that request's sequence id (`INT64_MIN` from a
/// backend that assigns none), `reply` the whole CDR frame (decode it with
/// [`nros_cpp_action_cancel_reply_decode`]) and `context` the one
/// [`nros_cpp_action_client_set_callbacks`] installed. Taking the reply is
/// what frees its request's slot (`MAX_CONCURRENT_GOALS` per client). NULL
/// uninstalls: replies then stay for
/// [`nros_cpp_action_client_take_cancel_reply`] /
/// [`nros_cpp_action_client_try_recv_cancel_response`].
///
/// # Safety
/// `handle` must be a valid action client storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_set_cancel_reply_callback(
    handle: *mut c_void,
    cb: Option<unsafe extern "C" fn(*const [u8; 16], i64, *const u8, usize, *mut c_void)>,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let client = unsafe { &mut *(handle as *mut CppActionClient) };
    client.callbacks.cancel_reply = cb;
    NROS_CPP_RET_OK
}

/// Decode a `CancelGoal` reply frame — issue 2034.
///
/// The frame a cancel-reply callback receives
/// ([`nros_cpp_action_client_set_cancel_reply_callback`]). Writes the RPC
/// return code to `return_code_out`, the `goals_canceling` count to
/// `count_out` and the first `min(count, infos_cap)` goals to `infos` (may be
/// NULL when `infos_cap` is 0). Returns `NROS_CPP_RET_OK`, or
/// `NROS_CPP_RET_ERROR` for a malformed frame (issue 0223's rule: a
/// truncation is not "no goals").
///
/// # Safety
/// `reply` must hold `reply_len` readable bytes; non-NULL out pointers must be
/// valid; `infos` must hold `infos_cap` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_cancel_reply_decode(
    reply: *const u8,
    reply_len: usize,
    return_code_out: *mut i8,
    infos: *mut nros_cpp_action_goal_info_t,
    infos_cap: usize,
    count_out: *mut usize,
) -> nros_cpp_ret_t {
    if reply.is_null()
        || return_code_out.is_null()
        || count_out.is_null()
        || (infos.is_null() && infos_cap != 0)
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let reply = unsafe { core::slice::from_raw_parts(reply, reply_len) };
    let (Some(code), Some(count)) = (
        action_frame::cancel_return_code(reply),
        action_frame::cancel_goals_count(reply),
    ) else {
        return NROS_CPP_RET_ERROR;
    };
    unsafe {
        *return_code_out = code;
        *count_out = count;
    }
    for i in 0..count.min(infos_cap) {
        if let Some((g, sec, nanosec)) = action_frame::cancel_goal_info(reply, i) {
            unsafe {
                *infos.add(i) = nros_cpp_action_goal_info_t {
                    uuid: g.uuid,
                    stamp_sec: sec,
                    stamp_nanosec: nanosec,
                };
            }
        }
    }
    NROS_CPP_RET_OK
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
    let cancel_reply_cb = client.callbacks.cancel_reply;
    let ctx = client.callbacks.context;
    let idx = client.arena_entry_index;
    let eptr = client.executor_ptr;

    let mut core = match unsafe { cpp_arena_core_mut(idx, eptr) } {
        Some(c) => c,
        None => return NROS_CPP_RET_OK,
    };

    // Poll goal acceptance reply. Issue 2009 — attributed to the goal its
    // request named, not to the most recently sent goal (`goal_counter`).
    if let Ok(Some((total_len, goal_id))) = core.try_recv_send_goal_reply_for()
        && let Some(cb) = goal_response_cb
    {
        let accepted = action_frame::goal_accepted(&core.result_buffer_ref()[..total_len]);
        unsafe { cb(accepted, &goal_id.uuid, ctx) };
    }

    // Poll feedback
    if let Ok(Some((goal_id, total_len))) = core.try_recv_feedback_raw()
        && let Some(cb) = feedback_cb
    {
        // Issue 2014 — offset and splice from `action_frame`, exactly as the
        // arena dispatch (`action_client_raw_try_process`) does, so a payload
        // reaches the callback in ONE framing whichever path took it off the
        // wire (issues #179, 2011).
        let mut spliced = [0u8; DEFAULT_RX_BUF_SIZE];
        if let Some(n) =
            action_frame::splice_feedback(&core.feedback_buffer_ref()[..total_len], &mut spliced)
        {
            unsafe { cb(&goal_id.uuid, spliced.as_ptr(), n, ctx) };
        }
    }

    // Poll result reply — `[encap][status i8][pad → 8][fields]`. Issue 2014:
    // status, offset and splice from `action_frame` (the offset-5 slice of
    // #179 handed the three pad bytes to the deserializer). Issue 2009 —
    // attributed like the acceptance above.
    if let Ok(Some((total_len, goal_id))) = core.try_recv_get_result_reply_for()
        && let Some(cb) = result_cb
    {
        let reply = &core.result_buffer_ref()[..total_len];
        let mut spliced = [0u8; DEFAULT_RX_BUF_SIZE];
        if let (Some(status), Some(n)) = (
            action_frame::result_status(reply),
            action_frame::splice_result(reply, &mut spliced),
        ) {
            unsafe { cb(&goal_id.uuid, status as i32, spliced.as_ptr(), n, ctx) };
        }
    }

    // Issue 2034 — a cancel reply, exactly as the executor's dispatch takes
    // it: only when the client has a cancel-reply callback to hand it to.
    if let Some(cb) = cancel_reply_cb
        && let Ok(Some((total_len, seq, goal_id))) = core.try_recv_cancel_reply_seq()
    {
        let reply = &core.result_buffer_ref()[..total_len];
        unsafe { cb(&goal_id.uuid, seq, reply.as_ptr(), reply.len(), ctx) };
    }

    NROS_CPP_RET_OK
}

// ============================================================================
// Phase 122.3.c.6.e — C-ABI wake-state slot. Same layout as
// `nros_c::service::rclc_wake_state_t` (16 bytes / 2 u64). Caller
// declares one per (entity, channel) pair.
// ============================================================================

#[repr(C)]
pub struct nros_cpp_wake_state_t {
    pub _opaque: [u64; 2],
}

#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_wake_state_get_zero_initialized() -> nros_cpp_wake_state_t {
    nros_cpp_wake_state_t { _opaque: [0u64; 2] }
}

/// Shared body of every `nros_cpp_*_set_wake_callback`. Returns
/// `NROS_CPP_RET_UNSUPPORTED` when `register` reports the backend cannot wake
/// this entity — no per-entity new-data slot — rather than accepting a
/// callback that never fires (issue 1940).
unsafe fn install_waker_on(
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
    register: impl FnOnce(&core::task::Waker) -> bool,
) -> nros_cpp_ret_t {
    if state.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let state_ptr = state as *mut nros_node::c_waker::CWakeState;
    unsafe {
        core::ptr::write(
            state_ptr,
            nros_node::c_waker::CWakeState { fn_ptr: cb, ctx },
        );
        let waker = nros_node::c_waker::make_waker(state_ptr);
        // Issue 1940 — a backend with no per-entity wake says so; `OK` here
        // would promise a callback that never fires.
        if !register(&waker) {
            return NROS_CPP_RET_UNSUPPORTED;
        }
    }
    NROS_CPP_RET_OK
}

/// Phase 122.3.c.6.e — wake on subscription rx.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_subscription_set_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    use nros_rmw::Subscription;
    let sub = unsafe { &*(storage as *const nros::internals::RmwSubscriber) };
    unsafe { install_waker_on(state, cb, ctx, |w| sub.register_waker(w)) }
}

/// Phase 122.3.c.6.e — wake on service-server rx.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_server_set_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    use nros_rmw::ServiceTrait;
    let srv = unsafe { &*(storage as *const nros::internals::RmwServiceServer) };
    unsafe { install_waker_on(state, cb, ctx, |w| srv.register_waker(w)) }
}

/// Phase 122.3.c.6.e — wake on service-client reply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_service_client_set_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    use nros_rmw::ClientTrait;
    let cli = unsafe { &*(storage as *const nros::internals::RmwServiceClient) };
    unsafe { install_waker_on(state, cb, ctx, |w| cli.register_waker(w)) }
}

// Per-channel wake callbacks for L1 polling action server / client.
// `storage` is the POLLING `ActionServerCore` / `ActionClientCore`
// inline storage (i.e. what `_init_polling` wrote).

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_set_goal_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &*(storage as *const PollingActionServerCore) };
    unsafe { install_waker_on(state, cb, ctx, |w| core.register_goal_waker(w)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_set_cancel_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &*(storage as *const PollingActionServerCore) };
    unsafe { install_waker_on(state, cb, ctx, |w| core.register_cancel_waker(w)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_set_get_result_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &*(storage as *const PollingActionServerCore) };
    unsafe { install_waker_on(state, cb, ctx, |w| core.register_get_result_waker(w)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_set_goal_response_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &*(storage as *const PollingActionClientCore) };
    unsafe { install_waker_on(state, cb, ctx, |w| core.register_goal_response_waker(w)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_set_cancel_response_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &*(storage as *const PollingActionClientCore) };
    unsafe { install_waker_on(state, cb, ctx, |w| core.register_cancel_response_waker(w)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_set_result_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &*(storage as *const PollingActionClientCore) };
    unsafe { install_waker_on(state, cb, ctx, |w| core.register_result_waker(w)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_set_feedback_wake_callback(
    storage: *mut c_void,
    state: *mut nros_cpp_wake_state_t,
    cb: Option<unsafe extern "C" fn(*mut c_void)>,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &*(storage as *const PollingActionClientCore) };
    unsafe { install_waker_on(state, cb, ctx, |w| core.register_feedback_waker(w)) }
}

// ============================================================================
// Phase 122.3.d — Layer-1 polling-mode FFI for action server / client
// ============================================================================
//
// Mirrors the nros-c L1 polling surface (122.3.c.6.b) for callers
// that drive their own scheduler. `_init_polling` writes the raw
// `ActionServerCore` / `ActionClientCore` into caller-provided
// inline storage; raw methods delegate to the core. Storage size:
// `NROS_CPP_RAW_ACTION_{SERVER,CLIENT}_OPAQUE_U64S` (× 8 bytes).
//
// The existing L2 callback path (`nros_cpp_action_{server,client}_create`
// + executor registration) stays — callers pick at construction time.

pub(crate) type PollingActionServerCore =
    nros_node::ActionServerCore<DEFAULT_RX_BUF_SIZE, DEFAULT_RX_BUF_SIZE, DEFAULT_RX_BUF_SIZE, 4>;

pub(crate) type PollingActionClientCore =
    nros_node::ActionClientCore<DEFAULT_RX_BUF_SIZE, DEFAULT_RX_BUF_SIZE, DEFAULT_RX_BUF_SIZE>;

/// Phase 122.3.d — initialize an L1 polling-mode action server.
///
/// Builds the 5 channels via the node's session and writes the
/// `ActionServerCore` into `storage` (must be at least
/// `NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S × 8` bytes,
/// 8-byte-aligned).
///
/// # Safety
/// All pointers valid; `action_name` / `type_name` / `type_hash`
/// are valid null-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_init_polling(
    node: *const nros_cpp_node_t,
    action_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    hashes: *const nros_cpp_action_hashes_t,
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
    let action_str = match unsafe { cstr_to_str(action_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 1913 — each endpoint on its own hash (NULL falls back to `hash_str`).
    let hashes = unsafe { nros_cpp_action_hashes_t::resolve(hashes, hash_str) };
    let node_name_str = core::str::from_utf8(&node_ref.name)
        .ok()
        .and_then(|s| s.split('\0').next());
    let ns_str = core::str::from_utf8(&node_ref.namespace)
        .ok()
        .and_then(|s| s.split('\0').next())
        .unwrap_or("/");
    let mut ctx_ref = unsafe { crate::cpp_ctx_locked(node_ref.executor) };
    let ctx: &mut crate::CppContext = &mut ctx_ref;

    // phase-428 W5 finding 3, swept — the CALLBACK action path resolves
    // (`:902`); these polling ones created from the raw name, so a `~` or a
    // launch remap moved the callback-mode action and not this one. All five
    // channel keyexprs derive from `action_info`, so resolving once here moves
    // the whole action.
    let resolved_act = match crate::resolve_node_entity_name(ctx, node_ref, action_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    use nros_node::{ActionInfo, QoSProfile, ServiceInfo, Session, TopicInfo};
    let action_info =
        ActionInfo::new(resolved_act.as_str(), type_str, hash_str).with_domain(ctx.domain_id);
    let session = ctx.executor.session_mut();

    fn with_node<'a>(info: ServiceInfo<'a>, node_name: Option<&'a str>) -> ServiceInfo<'a> {
        match node_name {
            Some(n) if !n.is_empty() => info.with_node_name(n),
            _ => info,
        }
    }

    // Issue 0454 / phase-354 W3 — per-CHANNEL DDS type, not the bare action
    // type. The type name is baked into the keyexpr, so `…Fibonacci_` where ROS
    // 2 expects `…Fibonacci_SendGoal_` makes query and queryable different keys
    // and every goal times out. Same defect phase-338 W3 fixed on the raw
    // register path; these polling arms had no caller to expose it.
    let send_goal_keyexpr: nros::heapless::String<256> = action_info.send_goal_key();
    let send_goal_type: nros::heapless::String<256> =
        nros::action_channel_type(type_str, "SendGoal");
    let send_goal_info = with_node(
        ServiceInfo::new(&send_goal_keyexpr, &send_goal_type, hashes.send_goal)
            .with_domain(ctx.domain_id)
            .with_namespace(ns_str),
        node_name_str,
    );
    let send_goal_server =
        match session.create_service(&send_goal_info, QoSProfile::services_default()) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let cancel_goal_keyexpr: nros::heapless::String<256> = action_info.cancel_goal_key();
    let cancel_goal_info = with_node(
        ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            hashes.cancel_goal,
        )
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str),
        node_name_str,
    );
    let cancel_goal_server =
        match session.create_service(&cancel_goal_info, QoSProfile::services_default()) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let get_result_keyexpr: nros::heapless::String<256> = action_info.get_result_key();
    let get_result_type: nros::heapless::String<256> =
        nros::action_channel_type(type_str, "GetResult");
    let get_result_info = with_node(
        ServiceInfo::new(&get_result_keyexpr, &get_result_type, hashes.get_result)
            .with_domain(ctx.domain_id)
            .with_namespace(ns_str),
        node_name_str,
    );
    let get_result_server =
        match session.create_service(&get_result_info, QoSProfile::services_default()) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let feedback_keyexpr: nros::heapless::String<256> = action_info.feedback_key();
    let feedback_type: nros::heapless::String<256> =
        nros::action_channel_type(type_str, "FeedbackMessage");
    let mut feedback_topic = TopicInfo::new(&feedback_keyexpr, &feedback_type, hashes.feedback)
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str);
    if let Some(n) = node_name_str
        && !n.is_empty()
    {
        feedback_topic = feedback_topic.with_node_name(n);
    }
    let feedback_publisher =
        match session.create_publisher(&feedback_topic, QoSProfile::BEST_EFFORT) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let status_keyexpr: nros::heapless::String<256> = action_info.status_key();
    let mut status_topic = TopicInfo::new(
        &status_keyexpr,
        "action_msgs::msg::dds_::GoalStatusArray_",
        hashes.status,
    )
    .with_domain(ctx.domain_id)
    .with_namespace(ns_str);
    if let Some(n) = node_name_str
        && !n.is_empty()
    {
        status_topic = status_topic.with_node_name(n);
    }
    let status_publisher = match session.create_publisher(&status_topic, QoSProfile::BEST_EFFORT) {
        Ok(h) => h,
        Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
    };

    let core = PollingActionServerCore::from_channels(
        send_goal_server,
        cancel_goal_server,
        get_result_server,
        feedback_publisher,
        status_publisher,
    );
    unsafe {
        core::ptr::write(storage as *mut PollingActionServerCore, core);
    }
    NROS_CPP_RET_OK
}

/// Phase 122.3.d — L1 polling: try to receive a goal request.
///
/// On success copies the goal payload (CDR-framing-stripped) into
/// `buf`, fills `goal_id_out` (16 bytes) + `sequence_number_out`,
/// returns bytes copied. `0` means no request pending; negative on
/// error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_try_recv_goal_request_raw(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
    sequence_number_out: *mut i64,
) -> i32 {
    if storage.is_null()
        || (buf.is_null() && buf_len != 0)
        || goal_id_out.is_null()
        || sequence_number_out.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    match core.try_recv_goal_request() {
        Ok(Some(req)) => {
            let goal_buf = core.goal_buffer();
            let payload_offset = action_frame::GOAL_REQUEST_PAYLOAD_OFFSET;
            if payload_offset + req.data_len > goal_buf.len() {
                return NROS_CPP_RET_ERROR;
            }
            let copy_len = req.data_len.min(buf_len);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    goal_buf.as_ptr().add(payload_offset),
                    buf,
                    copy_len,
                );
                (*goal_id_out).copy_from_slice(&req.goal_id.uuid);
                *sequence_number_out = req.sequence_number;
            }
            copy_len as i32
        }
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: accept a goal received via
/// `try_recv_goal_request_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_accept_goal_raw(
    storage: *mut c_void,
    goal_id: *const [u8; 16],
    sequence_number: i64,
) -> nros_cpp_ret_t {
    if storage.is_null() || goal_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    let id = GoalId {
        uuid: unsafe { *goal_id },
    };
    match core.accept_goal(id, sequence_number) {
        Ok(_) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: reject a goal.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_reject_goal_raw(
    storage: *mut c_void,
    sequence_number: i64,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    match core.reject_goal(sequence_number) {
        Ok(_) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: publish feedback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_publish_feedback_raw(
    storage: *mut c_void,
    goal_id: *const [u8; 16],
    feedback_cdr: *const u8,
    feedback_len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || goal_id.is_null() || (feedback_cdr.is_null() && feedback_len != 0) {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    let id = GoalId {
        uuid: unsafe { *goal_id },
    };
    let slice = unsafe { core::slice::from_raw_parts(feedback_cdr, feedback_len) };
    match core.publish_feedback_raw(&id, slice) {
        Ok(_) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: mark a goal terminal.
/// `status_code`: 4 = Succeeded, 5 = Canceled, 6 = Aborted (matches
/// the `nros_core::GoalStatus` discriminants).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_complete_goal_raw(
    storage: *mut c_void,
    goal_id: *const [u8; 16],
    status_code: i32,
    result_cdr: *const u8,
    result_len: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || goal_id.is_null() || (result_cdr.is_null() && result_len != 0) {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    let id = GoalId {
        uuid: unsafe { *goal_id },
    };
    let slice = unsafe { core::slice::from_raw_parts(result_cdr, result_len) };
    let status = match status_code {
        4 => nros::GoalStatus::Succeeded,
        5 => nros::GoalStatus::Canceled,
        6 => nros::GoalStatus::Aborted,
        _ => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // issue 0796 — see `nros_cpp_action_server_complete_goal`.
    match core.complete_goal_raw(&id, status, slice) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.c.6.d / .d — L1 polling: peek a pending
/// cancel-goal request. Writes goal_id + sequence_number +
/// current_status (matches `nros_cpp_goal_status_t` discriminants).
/// Returns `1` on peek, `0` if none pending, negative on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_try_recv_cancel_request_raw(
    storage: *mut c_void,
    goal_id_out: *mut [u8; 16],
    sequence_number_out: *mut i64,
    current_status_out: *mut i8,
) -> i32 {
    if storage.is_null()
        || goal_id_out.is_null()
        || sequence_number_out.is_null()
        || current_status_out.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    match core.try_recv_cancel_request() {
        Ok(Some(req)) => {
            unsafe {
                (*goal_id_out).copy_from_slice(&req.goal_id.uuid);
                *sequence_number_out = req.sequence_number;
                *current_status_out = req.current_status as i8;
            }
            1
        }
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.c.6.d / .d — L1 polling: reply to a cancel-goal
/// request. `return_code` matches `nros_core::CancelReturnCode`
/// (0 = Ok, 1 = Rejected, 2 = UnknownGoal, 3 = GoalTerminated) — the
/// `action_msgs/srv/CancelGoal` RPC status, NOT the per-goal
/// `rclcpp_action::CancelResponse` accept/reject a cancel callback returns (issue
/// 0796; the two used to share a name and their discriminants overlap with
/// opposite meanings).
/// `accepted` points to `accepted_count` 16-byte goal-id arrays.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_send_cancel_reply_raw(
    storage: *mut c_void,
    sequence_number: i64,
    return_code: i8,
    accepted: *const [u8; 16],
    accepted_count: usize,
) -> nros_cpp_ret_t {
    if storage.is_null() || (accepted.is_null() && accepted_count != 0) {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    let resp = match return_code {
        0 => nros_node::CancelReturnCode::Ok,
        1 => nros_node::CancelReturnCode::Rejected,
        2 => nros_node::CancelReturnCode::UnknownGoal,
        3 => nros_node::CancelReturnCode::GoalTerminated,
        _ => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let mut ids: nros::heapless::Vec<nros::GoalId, 8> = nros::heapless::Vec::new();
    for i in 0..accepted_count {
        if i >= 8 {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let uuid = unsafe { *accepted.add(i) };
        if ids.push(nros::GoalId { uuid }).is_err() {
            return NROS_CPP_RET_ERROR;
        }
    }
    match core.send_cancel_reply(sequence_number, resp, &ids) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: serve a pending get_result query.
/// Returns `1` if served, `0` if none pending, negative on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_try_handle_get_result_raw(
    storage: *mut c_void,
    default_result_cdr: *const u8,
    default_result_len: usize,
) -> i32 {
    if storage.is_null() || (default_result_cdr.is_null() && default_result_len != 0) {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionServerCore) };
    let data = unsafe { core::slice::from_raw_parts(default_result_cdr, default_result_len) };
    let slice = strip_cdr_header(data);
    match core.try_handle_get_result_raw(slice) {
        Ok(Some(_)) => 1,
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: drop the inline `ActionServerCore`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_server_destroy_polling(
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut PollingActionServerCore);
    }
    NROS_CPP_RET_OK
}

// ----------------------------------------------------------------------------
// Action client L1 polling
// ----------------------------------------------------------------------------

/// Phase 122.3.d — initialize an L1 polling-mode action client.
///
/// Builds the 3 service clients + feedback subscriber via the
/// session and writes the `ActionClientCore` into `storage`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_init_polling(
    node: *const nros_cpp_node_t,
    action_name: *const c_char,
    type_name: *const c_char,
    type_hash: *const c_char,
    hashes: *const nros_cpp_action_hashes_t,
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
    let action_str = match unsafe { cstr_to_str(action_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let type_str = match unsafe { cstr_to_str(type_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 0312 — an empty hash would land in the liveliness keyexpr as an
    // empty segment, making the entity invisible to ROS 2 discovery.
    let hash_str = match unsafe { cstr_to_str(type_hash) } {
        Some(s) => crate::normalize_type_hash(s),
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // Issue 1913 — each endpoint on its own hash (NULL falls back to `hash_str`).
    let hashes = unsafe { nros_cpp_action_hashes_t::resolve(hashes, hash_str) };
    let node_name_str = core::str::from_utf8(&node_ref.name)
        .ok()
        .and_then(|s| s.split('\0').next());
    let ns_str = core::str::from_utf8(&node_ref.namespace)
        .ok()
        .and_then(|s| s.split('\0').next())
        .unwrap_or("/");
    let mut ctx_ref = unsafe { crate::cpp_ctx_locked(node_ref.executor) };
    let ctx: &mut crate::CppContext = &mut ctx_ref;

    // phase-428 W5 finding 3, swept — the CALLBACK action path resolves
    // (`:902`); these polling ones created from the raw name, so a `~` or a
    // launch remap moved the callback-mode action and not this one. All five
    // channel keyexprs derive from `action_info`, so resolving once here moves
    // the whole action.
    let resolved_act = match crate::resolve_node_entity_name(ctx, node_ref, action_str) {
        Ok(r) => r,
        Err(()) => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    use nros_node::{ActionInfo, QoSProfile, ServiceInfo, Session, TopicInfo};
    let action_info =
        ActionInfo::new(resolved_act.as_str(), type_str, hash_str).with_domain(ctx.domain_id);
    let session = ctx.executor.session_mut();

    fn with_node<'a>(info: ServiceInfo<'a>, node_name: Option<&'a str>) -> ServiceInfo<'a> {
        match node_name {
            Some(n) if !n.is_empty() => info.with_node_name(n),
            _ => info,
        }
    }

    // Issue 0454 / phase-354 W3 — per-CHANNEL DDS type, not the bare action
    // type. The type name is baked into the keyexpr, so `…Fibonacci_` where ROS
    // 2 expects `…Fibonacci_SendGoal_` makes query and queryable different keys
    // and every goal times out. Same defect phase-338 W3 fixed on the raw
    // register path; these polling arms had no caller to expose it.
    let send_goal_keyexpr: nros::heapless::String<256> = action_info.send_goal_key();
    let send_goal_type: nros::heapless::String<256> =
        nros::action_channel_type(type_str, "SendGoal");
    let send_goal_info = with_node(
        ServiceInfo::new(&send_goal_keyexpr, &send_goal_type, hashes.send_goal)
            .with_domain(ctx.domain_id)
            .with_namespace(ns_str),
        node_name_str,
    );
    let send_goal_client =
        match session.create_client(&send_goal_info, QoSProfile::services_default()) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let cancel_goal_keyexpr: nros::heapless::String<256> = action_info.cancel_goal_key();
    let cancel_goal_info = with_node(
        ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            hashes.cancel_goal,
        )
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str),
        node_name_str,
    );
    let cancel_goal_client =
        match session.create_client(&cancel_goal_info, QoSProfile::services_default()) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let get_result_keyexpr: nros::heapless::String<256> = action_info.get_result_key();
    let get_result_type: nros::heapless::String<256> =
        nros::action_channel_type(type_str, "GetResult");
    let get_result_info = with_node(
        ServiceInfo::new(&get_result_keyexpr, &get_result_type, hashes.get_result)
            .with_domain(ctx.domain_id)
            .with_namespace(ns_str),
        node_name_str,
    );
    let get_result_client =
        match session.create_client(&get_result_info, QoSProfile::services_default()) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let feedback_keyexpr: nros::heapless::String<256> = action_info.feedback_key();
    let feedback_type: nros::heapless::String<256> =
        nros::action_channel_type(type_str, "FeedbackMessage");
    let mut feedback_topic = TopicInfo::new(&feedback_keyexpr, &feedback_type, hashes.feedback)
        .with_domain(ctx.domain_id)
        .with_namespace(ns_str);
    if let Some(n) = node_name_str
        && !n.is_empty()
    {
        feedback_topic = feedback_topic.with_node_name(n);
    }
    let feedback_subscriber =
        match session.create_subscription(&feedback_topic, QoSProfile::BEST_EFFORT) {
            Ok(h) => h,
            Err(_) => return NROS_CPP_RET_TRANSPORT_ERROR,
        };

    let core = PollingActionClientCore::new(
        send_goal_client,
        cancel_goal_client,
        get_result_client,
        feedback_subscriber,
    );
    unsafe {
        core::ptr::write(storage as *mut PollingActionClientCore, core);
    }
    NROS_CPP_RET_OK
}

/// Phase 122.3.d — L1 polling: send a goal. Writes 16-byte UUID
/// into `goal_id_out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_send_goal_raw(
    storage: *mut c_void,
    goal_cdr: *const u8,
    goal_len: usize,
    goal_id_out: *mut [u8; 16],
) -> nros_cpp_ret_t {
    if storage.is_null() || goal_id_out.is_null() || (goal_cdr.is_null() && goal_len != 0) {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionClientCore) };
    // Issue 0454 / phase-354 W3 — STRIP the encapsulation header.
    //
    // The parameter is `goal_cdr`: a C caller supplies CDR, which begins
    // with the 4-byte encapsulation header. `PollingActionClientCore::
    // send_goal_raw` wants goal FIELDS — its own doc says "without GoalId
    // framing", and it appends the bytes after a header `tx_writer`
    // (`CdrWriter::new_with_header`) has already written. Passing CDR
    // through unstripped therefore put TWO headers on the wire, which is
    // issue 0448's defect reaching a peer.
    //
    // The two INTERNAL callers already did this (`strip_cdr_header(goal_data)`
    // -> `goal_fields`); only the FFI arms did not, so the parameter name
    // stated one contract and the code implemented another.
    let slice = unsafe { core::slice::from_raw_parts(goal_cdr, goal_len) };
    match core.send_goal_raw(strip_cdr_header(slice)) {
        Ok(id) => {
            unsafe {
                (*goal_id_out).copy_from_slice(&id.uuid);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: try receiving the send_goal RPC reply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_goal_response_raw(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let mut goal_id = [0u8; 16];
    unsafe {
        nros_cpp_action_client_try_recv_goal_response_raw_for(storage, buf, buf_len, &mut goal_id)
    }
}

/// Issue 2011 — [`nros_cpp_action_client_try_recv_goal_response_raw`] that also writes the goal the reply is ABOUT (the
/// goal of the request whose sequence id it names, issue 2009) into
/// `goal_id_out`. With several goals in flight the bytes alone cannot say
/// which one a reply answers. A reply naming no pending request (an abandoned
/// one's) is dropped, as before. `goal_id_out` is written only when a reply is
/// returned (`> 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_goal_response_raw_for(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if storage.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionClientCore) };
    match core.try_recv_send_goal_reply_for() {
        Ok(Some((len, id))) => {
            let copy_len = len.min(buf_len);
            unsafe {
                core::ptr::copy_nonoverlapping(core.result_buffer_ref().as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
            }
            copy_len as i32
        }
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: send a get_result request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_send_get_result_request_raw(
    storage: *mut c_void,
    goal_id: *const [u8; 16],
) -> nros_cpp_ret_t {
    if storage.is_null() || goal_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionClientCore) };
    let id = GoalId {
        uuid: unsafe { *goal_id },
    };
    match core.send_get_result_request(&id) {
        Ok(_) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: try receiving the get_result reply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_result_raw(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let mut goal_id = [0u8; 16];
    unsafe { nros_cpp_action_client_try_recv_result_raw_for(storage, buf, buf_len, &mut goal_id) }
}

/// Issue 2011 — [`nros_cpp_action_client_try_recv_result_raw`] that also writes the goal the reply is ABOUT (the
/// goal of the request whose sequence id it names, issue 2009) into
/// `goal_id_out`. With several goals in flight the bytes alone cannot say
/// which one a reply answers. A reply naming no pending request (an abandoned
/// one's) is dropped, as before. `goal_id_out` is written only when a reply is
/// returned (`> 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_result_raw_for(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if storage.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionClientCore) };
    match core.try_recv_get_result_reply_for() {
        Ok(Some((len, id))) => {
            let copy_len = len.min(buf_len);
            unsafe {
                core::ptr::copy_nonoverlapping(core.result_buffer_ref().as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
            }
            copy_len as i32
        }
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: send a cancel request.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_send_cancel_request_raw(
    storage: *mut c_void,
    goal_id: *const [u8; 16],
) -> nros_cpp_ret_t {
    if storage.is_null() || goal_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionClientCore) };
    let id = GoalId {
        uuid: unsafe { *goal_id },
    };
    match core.send_cancel_request(&id) {
        Ok(_) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d / .c.6.c — L1 polling: try receiving the cancel
/// RPC reply.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_cancel_response_raw(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let mut goal_id = [0u8; 16];
    unsafe {
        nros_cpp_action_client_try_recv_cancel_response_raw_for(storage, buf, buf_len, &mut goal_id)
    }
}

/// Issue 2011 — [`nros_cpp_action_client_try_recv_cancel_response_raw`] that also writes the goal the reply is ABOUT (the
/// goal of the request whose sequence id it names, issue 2009) into
/// `goal_id_out`. With several goals in flight the bytes alone cannot say
/// which one a reply answers. A reply naming no pending request (an abandoned
/// one's) is dropped, as before. `goal_id_out` is written only when a reply is
/// returned (`> 0`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_cancel_response_raw_for(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if storage.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionClientCore) };
    match core.try_recv_cancel_reply_for() {
        Ok(Some((len, id))) => {
            let copy_len = len.min(buf_len);
            unsafe {
                core::ptr::copy_nonoverlapping(core.result_buffer_ref().as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
            }
            copy_len as i32
        }
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: try receiving feedback. Writes
/// goal_id_out + bytes copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_try_recv_feedback_raw(
    storage: *mut c_void,
    buf: *mut u8,
    buf_len: usize,
    goal_id_out: *mut [u8; 16],
) -> i32 {
    if storage.is_null() || (buf.is_null() && buf_len != 0) || goal_id_out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let core = unsafe { &mut *(storage as *mut PollingActionClientCore) };
    match core.try_recv_feedback_raw() {
        Ok(Some((id, len))) => {
            let copy_len = len.min(buf_len);
            unsafe {
                core::ptr::copy_nonoverlapping(core.feedback_buffer_ref().as_ptr(), buf, copy_len);
                (*goal_id_out).copy_from_slice(&id.uuid);
            }
            copy_len as i32
        }
        Ok(None) => 0,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Phase 122.3.d — L1 polling: drop the inline `ActionClientCore`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_action_client_destroy_polling(
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut PollingActionClientCore);
    }
    NROS_CPP_RET_OK
}
