//! Phase 212.M-F.4.c — C++ mirror of the Rust `TickCtx` client-side seams.
//!
//! The Rust substrate [`nros::TickCtx`] grew two client-side methods in
//! Phase 212.M-F.4 (`d15565efe`):
//!
//! - [`TickCtx::call_raw`] / [`TickCtx::call`]      — drive a service-client
//!   request from a tick body, where the executor is free.
//! - [`TickCtx::send_goal_raw`] / [`TickCtx::send_goal`] — kick an
//!   action-client goal from a tick body; the assigned [`GoalId`] is
//!   stamped by the server-side accept.
//!
//! Both seams route through the [`nros::ClientDispatch`] trait so the
//! generated runtime (in `nros-cli`'s codegen — Phase 212.M-F.4.a, not yet
//! shipped) can resolve client handles by stable entity id without dragging
//! the executor type into [`TickCtx`].
//!
//! This module exposes the symmetric FFI to the C++ wrapper in
//! `<nros/tick_ctx.hpp>`. The two `extern "C"` entry points,
//! `nros_cpp_tick_ctx_call_raw` and `nros_cpp_tick_ctx_send_goal_raw`,
//! follow the action-server pattern from `src/action.rs`: the C++ side
//! passes an opaque per-tick context pointer (provided by the future
//! generated runtime), CDR bytes in, and either a response buffer +
//! out-length (call) or a 16-byte goal id (send_goal) out.
//!
//! ## Contract (M-F.4.c)
//!
//! Until the codegen-side `GenClientDispatch` impl lands (M-F.4.a) and the
//! generated runtime starts passing a real non-null tick-ctx handle, both
//! FFI symbols return `NROS_CPP_RET_ERROR` — matching the
//! [`UnsupportedClients`] stub on the Rust side
//! (`ComponentError::Runtime` for every method). The symbols exist + are
//! callable; the runtime returns an error until codegen + BSP plumbing
//! reach them. C++ user code can write tick bodies against the typed
//! `TickCtx::call<Req, Resp>()` / `TickCtx::send_goal<G>()` wrappers today;
//! the call / send_goal will fail at runtime with `ErrorCode::Error` until
//! the generated runtime ships.

use core::ffi::c_void;

use crate::{NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK, nros_cpp_ret_t};

/// Issue a service-client raw-CDR request from a tick body and block on
/// the reply (Phase 212.M-F.4.c).
///
/// Mirrors `nros::TickCtx::call_raw` (Rust). The C++ side passes the
/// opaque per-tick handle it received from the generated runtime as
/// `tick_ctx`, plus the request CDR bytes + a response buffer the runtime
/// will fill with the reply CDR. On success `*response_len_out` carries
/// the response length in bytes; on error it is left untouched.
///
/// # Parameters
/// * `tick_ctx` — opaque per-tick context handle. Provided by the
///   generated runtime (M-F.4.a, not yet shipped); pass any non-null
///   pointer for forward-compat smoke tests — the call will still fail
///   with `NROS_CPP_RET_ERROR` until the runtime backs the handle.
/// * `service_entity` — stable entity id of the service client (NUL-term).
/// * `service_entity_len` — `service_entity` byte length (excluding NUL).
/// * `request_cdr` / `request_len` — request CDR bytes.
/// * `response_buf` / `response_buf_cap` — caller-owned reply buffer.
/// * `response_len_out` — out-param: response length on success.
///
/// # Safety
/// All non-NULL pointers must be valid for the indicated lengths. NULL
/// `service_entity`, NULL `request_cdr` with non-zero `request_len`,
/// NULL `response_buf` with non-zero `response_buf_cap`, or NULL
/// `response_len_out` all yield `NROS_CPP_RET_INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_tick_ctx_call_raw(
    tick_ctx: *mut c_void,
    service_entity: *const u8,
    service_entity_len: usize,
    request_cdr: *const u8,
    request_len: usize,
    response_buf: *mut u8,
    response_buf_cap: usize,
    response_len_out: *mut usize,
) -> nros_cpp_ret_t {
    if service_entity.is_null()
        || service_entity_len == 0
        || (request_cdr.is_null() && request_len != 0)
        || (response_buf.is_null() && response_buf_cap != 0)
        || response_len_out.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    // M-F.4.c contract: until the codegen-side `GenClientDispatch` impl
    // lands (M-F.4.a) and the generated runtime passes a real non-null
    // tick-ctx handle, return `NROS_CPP_RET_ERROR` for every well-formed
    // call. Matches the `UnsupportedClients` stub in `nros-node`'s
    // `ExecutorComponentRuntime::run_ticks` which routes every method to
    // `ComponentError::Runtime`. The symbol exists + is callable; the
    // runtime returns an error until plumbing reaches it.
    let _ = tick_ctx;
    let _ = (request_cdr, request_len);
    let _ = (response_buf, response_buf_cap);
    NROS_CPP_RET_ERROR
}

/// Kick an action-client goal from a tick body (Phase 212.M-F.4.c).
///
/// Mirrors `nros::TickCtx::send_goal_raw` (Rust). The server-side
/// accept stamps the assigned [`GoalId`]; this call returns that id in
/// `goal_id_out` (16 bytes). Result + feedback streams arrive via
/// callback dispatch — not this method.
///
/// # Parameters
/// * `tick_ctx` — opaque per-tick context handle (see `call_raw`).
/// * `action_entity` / `action_entity_len` — stable action-client entity id.
/// * `goal_cdr` / `goal_len` — goal request CDR bytes.
/// * `goal_id_out` — 16-byte buffer; receives the server-stamped goal id.
///
/// # Safety
/// All non-NULL pointers must be valid for the indicated lengths. NULL
/// `action_entity`, NULL `goal_cdr` with non-zero `goal_len`, or NULL
/// `goal_id_out` yield `NROS_CPP_RET_INVALID_ARGUMENT`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_tick_ctx_send_goal_raw(
    tick_ctx: *mut c_void,
    action_entity: *const u8,
    action_entity_len: usize,
    goal_cdr: *const u8,
    goal_len: usize,
    goal_id_out: *mut [u8; 16],
) -> nros_cpp_ret_t {
    if action_entity.is_null()
        || action_entity_len == 0
        || (goal_cdr.is_null() && goal_len != 0)
        || goal_id_out.is_null()
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    // M-F.4.c contract: stub-error until codegen lands (see call_raw above).
    let _ = tick_ctx;
    let _ = (goal_cdr, goal_len);
    NROS_CPP_RET_ERROR
}

// Suppress "unused constant" warning when only the error path is exercised.
#[allow(dead_code)]
const _NROS_CPP_TICK_CTX_OK_REF: nros_cpp_ret_t = NROS_CPP_RET_OK;

// ============================================================================
// Tests — exercise the stub FFI surface (Phase 212.M-F.4.c)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    const SERVICE: &[u8] = b"my_service";
    const ACTION: &[u8] = b"my_action";

    #[test]
    fn call_raw_rejects_null_service_entity() {
        let mut resp = [0u8; 16];
        let mut resp_len: usize = 0;
        let ret = unsafe {
            nros_cpp_tick_ctx_call_raw(
                ptr::null_mut(),
                ptr::null(),
                0,
                ptr::null(),
                0,
                resp.as_mut_ptr(),
                resp.len(),
                &mut resp_len,
            )
        };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn call_raw_rejects_null_response_len_out() {
        let req = [0u8; 8];
        let mut resp = [0u8; 16];
        let ret = unsafe {
            nros_cpp_tick_ctx_call_raw(
                ptr::null_mut(),
                SERVICE.as_ptr(),
                SERVICE.len(),
                req.as_ptr(),
                req.len(),
                resp.as_mut_ptr(),
                resp.len(),
                ptr::null_mut(),
            )
        };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn call_raw_rejects_request_ptr_null_with_nonzero_len() {
        let mut resp = [0u8; 16];
        let mut resp_len: usize = 0;
        let ret = unsafe {
            nros_cpp_tick_ctx_call_raw(
                ptr::null_mut(),
                SERVICE.as_ptr(),
                SERVICE.len(),
                ptr::null(),
                4,
                resp.as_mut_ptr(),
                resp.len(),
                &mut resp_len,
            )
        };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn call_raw_well_formed_returns_stub_error() {
        // M-F.4.c stub contract: well-formed call with a non-null
        // (sentinel) tick handle returns RET_ERROR until codegen lands.
        let req = [0u8; 8];
        let mut resp = [0u8; 16];
        let mut resp_len: usize = 0xdead;
        let sentinel = 0xdeadbeefusize as *mut c_void;
        let ret = unsafe {
            nros_cpp_tick_ctx_call_raw(
                sentinel,
                SERVICE.as_ptr(),
                SERVICE.len(),
                req.as_ptr(),
                req.len(),
                resp.as_mut_ptr(),
                resp.len(),
                &mut resp_len,
            )
        };
        assert_eq!(ret, NROS_CPP_RET_ERROR);
        // The response length is left untouched on error.
        assert_eq!(resp_len, 0xdead);
    }

    #[test]
    fn send_goal_raw_rejects_null_action_entity() {
        let mut goal_id = [0u8; 16];
        let ret = unsafe {
            nros_cpp_tick_ctx_send_goal_raw(
                ptr::null_mut(),
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut goal_id,
            )
        };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn send_goal_raw_rejects_null_goal_id_out() {
        let goal = [0u8; 8];
        let ret = unsafe {
            nros_cpp_tick_ctx_send_goal_raw(
                ptr::null_mut(),
                ACTION.as_ptr(),
                ACTION.len(),
                goal.as_ptr(),
                goal.len(),
                ptr::null_mut(),
            )
        };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn send_goal_raw_rejects_goal_ptr_null_with_nonzero_len() {
        let mut goal_id = [0u8; 16];
        let ret = unsafe {
            nros_cpp_tick_ctx_send_goal_raw(
                ptr::null_mut(),
                ACTION.as_ptr(),
                ACTION.len(),
                ptr::null(),
                4,
                &mut goal_id,
            )
        };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn send_goal_raw_well_formed_returns_stub_error() {
        // M-F.4.c stub contract: well-formed call returns RET_ERROR until
        // the codegen-side `GenClientDispatch` impl lands.
        let goal = [0u8; 8];
        let mut goal_id = [0xffu8; 16];
        let sentinel = 0xdeadbeefusize as *mut c_void;
        let ret = unsafe {
            nros_cpp_tick_ctx_send_goal_raw(
                sentinel,
                ACTION.as_ptr(),
                ACTION.len(),
                goal.as_ptr(),
                goal.len(),
                &mut goal_id,
            )
        };
        assert_eq!(ret, NROS_CPP_RET_ERROR);
        // The goal-id buffer is left untouched on error.
        assert_eq!(goal_id, [0xffu8; 16]);
    }
}
