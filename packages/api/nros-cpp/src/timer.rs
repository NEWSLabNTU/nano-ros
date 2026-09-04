//! Timer FFI functions for the C++ API.
//!
//! Issue 0436 — user-supplied executor handles are tag-validated via
//! `cpp_ctx_checked` instead of being blind-cast to `*mut CppContext`.

use core::ffi::{c_char, c_void};

use nros_node::timer::TimerDuration;

use crate::{
    NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK,
    cpp_ctx_checked, cstr_to_str, nros_cpp_node_t, nros_cpp_ret_t,
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
    if out_handle_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let cb = match callback {
        Some(cb) => cb,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let c_context = context;

    let wrapper = move || unsafe {
        cb(c_context);
    };

    match ctx
        .executor
        .register_timer(TimerDuration::from_millis(period_ms), wrapper)
    {
        Ok(handle_id) => {
            unsafe {
                *out_handle_id = handle_id.0;
            }
            // phase-308 — timers never reach the RMW, so the recording backend
            // cannot see them; this hook is how they enter the sidecar. No-op
            // unless `metadata-mode` is on.
            crate::metadata_hooks::on_timer_create(period_ms);
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_FULL,
    }
}

/// Create a repeating timer driven by a CLOCK — phase-425 W4, the shape rclcpp
/// spells `create_timer(node, clock, period, cb)`.
///
/// `nros_cpp_timer_create` is the WALL timer: it advances with the executor's
/// monotonic spin delta and no simulator can slow it down. This one advances
/// with `clock_type`, so a `NROS_CLOCK_ROS_TIME` timer follows `/clock` — it
/// stops while the simulator is paused, and it tracks a bag's replay rate.
///
/// With no `/clock` source installed, a ROS-time timer reads system time, the
/// same fallback `rclcpp::Clock` has.
///
/// # Parameters
/// * `executor_handle` — Executor handle from `nros_cpp_init()`.
/// * `clock_type` — Which clock advances the timer.
/// * `period_ms` — Timer period in milliseconds, ON THAT CLOCK.
/// * `callback` — Function called when the timer fires.
/// * `context` — User context passed to the callback.
/// * `out_handle_id` — Receives the timer handle ID for cancel/reset.
///
/// # Safety
/// `executor_handle` and `out_handle_id` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_timer_create_on_clock(
    executor_handle: *mut c_void,
    clock_type: u8,
    period_ms: u64,
    callback: nros_cpp_timer_callback_t,
    context: *mut c_void,
    out_handle_id: *mut usize,
) -> nros_cpp_ret_t {
    use nros_c::nros_clock_type_t as Ct;
    use nros_node::executor::TimerClockSource;

    if out_handle_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    // `u8` and not `nros_clock_type_t`, deliberately: `nros_cpp_ffi.h` is
    // generated with `no_includes`, so a parameter typed with nros-c's enum
    // would name a type the header does not declare and every TU that includes
    // it without `nros/clock.h` fails to compile (action_client.hpp did). C++
    // converts the unscoped enum implicitly, so `clock.get_clock_type()` still
    // passes without a cast at the call site.
    //
    // An UNINITIALIZED clock is rejected rather than defaulted: a timer created
    // on a clock the caller never initialised would silently be a wall timer,
    // which is the exact confusion phase-425 exists to remove. An out-of-range
    // value is rejected for the same reason — the widening cannot be UB here,
    // unlike a transmute into the enum.
    let source = match clock_type {
        x if x == Ct::NROS_CLOCK_STEADY_TIME as u8 => TimerClockSource::Steady,
        x if x == Ct::NROS_CLOCK_ROS_TIME as u8 => TimerClockSource::Ros,
        x if x == Ct::NROS_CLOCK_SYSTEM_TIME as u8 => TimerClockSource::System,
        _ => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let cb = match callback {
        Some(cb) => cb,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let c_context = context;

    let wrapper = move || unsafe {
        cb(c_context);
    };

    match ctx.executor.register_timer_on_clock(
        TimerDuration::from_millis(period_ms),
        source,
        wrapper,
    ) {
        Ok(handle_id) => {
            unsafe {
                *out_handle_id = handle_id.0;
            }
            crate::metadata_hooks::on_timer_create(period_ms);
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
    if out_handle_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let cb = match callback {
        Some(cb) => cb,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let c_context = context;

    let wrapper = move || unsafe {
        cb(c_context);
    };

    match ctx
        .executor
        .register_timer_oneshot(TimerDuration::from_millis(delay_ms), wrapper)
    {
        Ok(handle_id) => {
            unsafe {
                *out_handle_id = handle_id.0;
            }
            // phase-308 — timers never reach the RMW, so the recording backend
            // cannot see them; this hook is how they enter the sidecar. No-op
            // unless `metadata-mode` is on.
            crate::metadata_hooks::on_timer_create(delay_ms);
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_FULL,
    }
}

/// Phase 273 (RFC-0047) — create a repeating timer **in** a named callback group.
///
/// Identical to `nros_cpp_timer_create` but additionally associates the timer
/// with a callback group. The executor resolves `(node, group_name)` via its
/// `group_sched_table` and binds the timer's callback to the group's
/// `SchedContext`. `callback_group` may be NULL or empty — both behave
/// identically to `nros_cpp_timer_create`.
///
/// # Parameters
/// * `executor_handle` — Executor handle from `nros_cpp_init()`.
/// * `node` — Node handle (`nros_cpp_node_t*`) this timer belongs to; used to
///   resolve the group binding. May be NULL (falls back to executor primary node).
/// * `period_ms` — Timer period in milliseconds.
/// * `callback` — Function called when the timer fires.
/// * `context` — User context passed to the callback.
/// * `callback_group` — Null-terminated group name, or NULL/empty for default.
/// * `out_handle_id` — Receives the timer handle ID.
///
/// # Safety
/// `executor_handle` and `out_handle_id` must be valid pointers.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn nros_cpp_timer_create_in_group(
    executor_handle: *mut c_void,
    node: *const nros_cpp_node_t,
    period_ms: u64,
    callback: nros_cpp_timer_callback_t,
    context: *mut c_void,
    callback_group: *const c_char,
    out_handle_id: *mut usize,
) -> nros_cpp_ret_t {
    if out_handle_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let cb = match callback {
        Some(cb) => cb,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let c_context = context;

    let wrapper = move || unsafe {
        cb(c_context);
    };

    // Resolve node_id from the node handle (None ⇒ primary/executor default).
    let node_id = if node.is_null() {
        None
    } else {
        let node_ref = unsafe { &*node };
        crate::node_id_opt(node_ref)
    };

    // Extract group name (NULL or empty ⇒ None ⇒ node default).
    let group_str = if callback_group.is_null() {
        None
    } else {
        let s = unsafe { cstr_to_str(callback_group) }.unwrap_or("");
        if s.is_empty() { None } else { Some(s) }
    };

    match ctx.executor.register_timer_on(
        node_id,
        TimerDuration::from_millis(period_ms),
        wrapper,
        group_str,
    ) {
        Ok(handle_id) => {
            unsafe {
                *out_handle_id = handle_id.0;
            }
            // phase-308 — timers never reach the RMW, so the recording backend
            // cannot see them; this hook is how they enter the sidecar. No-op
            // unless `metadata-mode` is on.
            crate::metadata_hooks::on_timer_create(period_ms);
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
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
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
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
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
pub unsafe extern "C" fn nros_cpp_timer_is_canceled(
    executor_handle: *mut c_void,
    handle_id: usize,
) -> bool {
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return true;
    };
    let ctx = &*ctx;
    let id = nros_node::HandleId(handle_id);
    ctx.executor.timer_is_canceled(id)
}
