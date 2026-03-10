//! Timer FFI functions for the C++ API.

use core::ffi::c_void;

use nros_node::timer::TimerDuration;

use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_OK, nros_cpp_ret_t,
};

/// C callback type for timers: `void callback(void* context)`.
pub type nros_cpp_timer_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Create a repeating timer and register it with the executor.
///
/// The timer fires every `period_ms` milliseconds during `spin_once()`.
///
/// # Parameters
/// * `executor_handle` — Executor handle from `nros_cpp_init()`.
/// * `period_ms` — Timer period in milliseconds.
/// * `callback` — Function called when the timer fires.
/// * `context` — User context passed to the callback.
/// * `out_handle_id` — Receives the timer handle ID for cancel/reset.
///
/// # Safety
/// `executor_handle` and `out_handle_id` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_timer_create(
    executor_handle: *mut c_void,
    period_ms: u64,
    callback: nros_cpp_timer_callback_t,
    context: *mut c_void,
    out_handle_id: *mut usize,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() || out_handle_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let cb = match callback {
        Some(cb) => cb,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let c_context = context;

    let wrapper = move || unsafe {
        cb(c_context);
    };

    match ctx
        .executor
        .add_timer(TimerDuration::from_millis(period_ms), wrapper)
    {
        Ok(handle_id) => {
            unsafe {
                *out_handle_id = handle_id.0;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_FULL,
    }
}

/// Create a one-shot timer and register it with the executor.
///
/// The timer fires once after `delay_ms` milliseconds during `spin_once()`.
///
/// # Parameters
/// * `executor_handle` — Executor handle from `nros_cpp_init()`.
/// * `delay_ms` — Delay in milliseconds before the timer fires.
/// * `callback` — Function called when the timer fires.
/// * `context` — User context passed to the callback.
/// * `out_handle_id` — Receives the timer handle ID.
///
/// # Safety
/// `executor_handle` and `out_handle_id` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_timer_create_oneshot(
    executor_handle: *mut c_void,
    delay_ms: u64,
    callback: nros_cpp_timer_callback_t,
    context: *mut c_void,
    out_handle_id: *mut usize,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() || out_handle_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let cb = match callback {
        Some(cb) => cb,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let c_context = context;

    let wrapper = move || unsafe {
        cb(c_context);
    };

    match ctx
        .executor
        .add_timer_oneshot(TimerDuration::from_millis(delay_ms), wrapper)
    {
        Ok(handle_id) => {
            unsafe {
                *out_handle_id = handle_id.0;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_FULL,
    }
}

/// Cancel a timer.
///
/// A cancelled timer stops firing but remains in the executor arena.
/// Use `nros_cpp_timer_reset()` to restart it.
///
/// # Safety
/// `executor_handle` must be a valid executor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_timer_cancel(
    executor_handle: *mut c_void,
    handle_id: usize,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let id = nros_node::HandleId(handle_id);

    match ctx.executor.cancel_timer(id) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Reset a timer (restart from zero elapsed time).
///
/// If the timer was cancelled, this also un-cancels it.
///
/// # Safety
/// `executor_handle` must be a valid executor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_timer_reset(
    executor_handle: *mut c_void,
    handle_id: usize,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let id = nros_node::HandleId(handle_id);

    match ctx.executor.reset_timer(id) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Check if a timer is cancelled.
///
/// # Safety
/// `executor_handle` must be a valid executor handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_timer_is_cancelled(
    executor_handle: *mut c_void,
    handle_id: usize,
) -> bool {
    if executor_handle.is_null() {
        return true;
    }

    let ctx = unsafe { &*(executor_handle as *const CppContext) };
    let id = nros_node::HandleId(handle_id);
    ctx.executor.timer_is_cancelled(id)
}
