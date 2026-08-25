//! Shutdown-hook FFI for the C++ API (issue 0790).
//!
//! rclcpp hangs these on `Context`, in two ORDERED phases: pre-shutdown
//! callbacks run before entities are torn down, on-shutdown callbacks after.
//! There is no `Context` here — phase-379's init stage records the collapse
//! into one support/executor object — so the executor owns them, which is also
//! what `close` / `fini` is called on.
//!
//! The pre-shutdown phase is the load-bearing half. On a desktop a process that
//! exits without ordered cleanup leaks nothing the OS will not reclaim; on a
//! device an actuator holds its last commanded position, a SPI or CAN
//! peripheral stays claimed and a DMA channel stays armed. Releasing those
//! needs the entities to still WORK, so the node can publish a final state or
//! answer a last request.
//!
//! Issue 0436 — the executor handle is tag-validated via `cpp_ctx_checked`
//! rather than blind-cast, like every other shim in this crate.

use core::ffi::c_void;

// Gated exactly like the four entry points below, and for the same reason: the
// module itself is UNGATED so the callback typedef and `NROS_CPP_MAX_SHUTDOWN_HOOKS`
// always reach the header, but everything imported here is reachable only from a
// `rmw-cffi` build (`cpp_ctx_checked` is itself gated). Without this the crate
// fails to compile under `--no-default-features`, which only
// `check-workspace-features` builds.
#[cfg(feature = "rmw-cffi")]
use crate::{
    NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_NOT_FOUND, NROS_CPP_RET_OK,
    cpp_ctx_checked, nros_cpp_ret_t,
};

/// C callback type for shutdown hooks: `void callback(void* context)`.
//
// A LOCAL alias, not `nros_node::ShutdownCallbackFn`: cbindgen runs with
// `parse_deps = false`, so a type owned by another crate in an `extern "C"`
// signature degrades to an opaque struct the C++ header cannot call through.
// Same reason `nros_cpp_timer_callback_t` is spelled in `timer.rs`.
pub type nros_cpp_shutdown_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Handle to a registered shutdown hook. Opaque to C++: pass it back to the
/// matching `remove`, or compare against
/// `NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID`.
pub type nros_cpp_shutdown_callback_handle_t = u32;

/// The value no successful registration ever produces.
//
// A LITERAL: cbindgen silently DROPS a constant whose initializer it cannot
// evaluate, and a constant missing from the header is worse than a wrong one —
// C++ compiles until somebody uses it. The `const _` below keeps it honest.
pub const NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID: nros_cpp_shutdown_callback_handle_t =
    0xFFFF_FFFF;

const _: () = assert!(
    NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID == nros_node::ShutdownCallbackHandle::INVALID.0,
    "the C++-visible invalid-handle literal drifted from nros_node's ShutdownCallbackHandle::INVALID"
);

/// Register a hook to run BEFORE the executor's session is closed.
///
/// On success writes the handle through `out_handle` and returns
/// `NROS_CPP_RET_OK`; `NROS_CPP_RET_FULL` when the phase's fixed table is
/// exhausted (`NROS_EXECUTOR_MAX_SHUTDOWN_CBS`, default 2).
///
/// # Safety
/// * `executor_handle` must be a handle from `nros_cpp_init()`.
/// * `out_handle` must be a valid pointer.
/// * `callback` must be safe to invoke once with `context`, and `context` must
///   stay valid until the hook runs or is removed.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_add_pre_shutdown_callback(
    executor_handle: *mut c_void,
    callback: nros_cpp_shutdown_callback_t,
    context: *mut c_void,
    out_handle: *mut nros_cpp_shutdown_callback_handle_t,
) -> nros_cpp_ret_t {
    unsafe { add_shutdown_callback(executor_handle, callback, context, out_handle, Phase::Pre) }
}

/// Register a hook to run AFTER the executor's session is closed.
///
/// rclcpp's `add_on_shutdown_callback` / `rclcpp::on_shutdown`. Entities are
/// gone by the time it runs, so anything that needs the wire belongs in
/// [`nros_cpp_add_pre_shutdown_callback`].
///
/// # Safety
/// Same contract as [`nros_cpp_add_pre_shutdown_callback`].
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_add_on_shutdown_callback(
    executor_handle: *mut c_void,
    callback: nros_cpp_shutdown_callback_t,
    context: *mut c_void,
    out_handle: *mut nros_cpp_shutdown_callback_handle_t,
) -> nros_cpp_ret_t {
    unsafe { add_shutdown_callback(executor_handle, callback, context, out_handle, Phase::Post) }
}

/// Remove a registered pre-shutdown hook.
///
/// `NROS_CPP_RET_OK` when `handle` named a live hook, `NROS_CPP_RET_NOT_FOUND`
/// when it did not — including an already-removed handle and one issued for the
/// OTHER phase (the phase is part of the handle, so it cannot cross over).
///
/// # Safety
/// `executor_handle` must be a handle from `nros_cpp_init()`.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_remove_pre_shutdown_callback(
    executor_handle: *mut c_void,
    handle: nros_cpp_shutdown_callback_handle_t,
) -> nros_cpp_ret_t {
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    if ctx
        .executor
        .remove_pre_shutdown_callback(nros_node::ShutdownCallbackHandle(handle))
    {
        NROS_CPP_RET_OK
    } else {
        NROS_CPP_RET_NOT_FOUND
    }
}

/// Remove a registered on-shutdown hook.
/// See [`nros_cpp_remove_pre_shutdown_callback`].
///
/// # Safety
/// `executor_handle` must be a handle from `nros_cpp_init()`.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_remove_on_shutdown_callback(
    executor_handle: *mut c_void,
    handle: nros_cpp_shutdown_callback_handle_t,
) -> nros_cpp_ret_t {
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    if ctx
        .executor
        .remove_on_shutdown_callback(nros_node::ShutdownCallbackHandle(handle))
    {
        NROS_CPP_RET_OK
    } else {
        NROS_CPP_RET_NOT_FOUND
    }
}

/// Which registration the shared body performs. Private — the C++ surface is
/// two named functions, matching rclcpp, rather than one with a mode argument.
#[cfg(feature = "rmw-cffi")]
enum Phase {
    Pre,
    Post,
}

/// # Safety
/// See [`nros_cpp_add_pre_shutdown_callback`].
#[cfg(feature = "rmw-cffi")]
unsafe fn add_shutdown_callback(
    executor_handle: *mut c_void,
    callback: nros_cpp_shutdown_callback_t,
    context: *mut c_void,
    out_handle: *mut nros_cpp_shutdown_callback_handle_t,
    phase: Phase,
) -> nros_cpp_ret_t {
    if out_handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(callback) = callback else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    // SAFETY: forwarded verbatim from this function's own contract.
    let registered = unsafe {
        match phase {
            Phase::Pre => ctx.executor.add_pre_shutdown_callback(callback, context),
            Phase::Post => ctx.executor.add_on_shutdown_callback(callback, context),
        }
    };
    match registered {
        Ok(handle) => {
            unsafe { *out_handle = handle.0 };
            NROS_CPP_RET_OK
        }
        Err(_) => {
            unsafe { *out_handle = NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID };
            NROS_CPP_RET_FULL
        }
    }
}
