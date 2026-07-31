//! Phase 269 (W0) — executor-shim: lifecycle FFI over the CppContext handle.
//!
//! Mirrors `nros-c/src/lifecycle.rs`'s service-backed module but recovers the
//! executor from `CppContext*` instead of `nros_executor_t*`. W1/W2 emitters
//! call these; no emitter wires them yet this wave.

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use core::ffi::c_void;

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use nros_node::LifecycleTransition;

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use nros_node::lifecycle::{LifecycleCallbackSlot, LifecycleError};

/// C callback type for a lifecycle transition: `uint8_t callback(void* context)`.
///
/// The return value is a REP-2002 `TransitionResult` (`Success = 0`, `Failure = 1`,
/// `Error = 2`). Registered per transition via `nros_cpp_lifecycle_register_on_*`;
/// all six callbacks on one node share the single `ctx` set by the last register call
/// (the C++ `LifecycleNode` wrapper always passes `this`).
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
pub type nros_cpp_lifecycle_callback_t = unsafe extern "C" fn(*mut c_void) -> u8;

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_NOT_INIT,
    NROS_CPP_RET_OK, nros_cpp_ret_t,
};

/// Register the five REP-2002 lifecycle services on the C++ executor's node.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*` produced by `nros_cpp_init`.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_register_lifecycle_services(
    executor: *mut c_void,
) -> nros_cpp_ret_t {
    if executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    match ctx.executor.register_lifecycle_services() {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Trigger a lifecycle transition on the C++ executor's state machine.
///
/// `transition_id` follows the REP-2002 numbering: Configure=1, Activate=2,
/// Deactivate=3, Cleanup=4, Shutdown=5, ErrorProcessed=6.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. Any registered transition
/// callbacks are invoked through raw function pointers; the caller must ensure
/// they and their captured context remain live.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_change_state(
    executor: *mut c_void,
    transition_id: u8,
) -> nros_cpp_ret_t {
    if executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    let Some(sm) = ctx.executor.lifecycle_state_machine_mut() else {
        return NROS_CPP_RET_NOT_INIT;
    };
    let Some(t) = LifecycleTransition::from_u8(transition_id) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    // SAFETY: forwarded through this function's unsafe contract.
    match unsafe { sm.trigger_transition(t) } {
        Ok(_) => NROS_CPP_RET_OK,
        Err(LifecycleError::NodeFinalized) => NROS_CPP_RET_ERROR,
        Err(LifecycleError::InvalidTransition { .. }) => NROS_CPP_RET_INVALID_ARGUMENT,
        Err(LifecycleError::CallbackFailed { .. }) => NROS_CPP_RET_ERROR,
    }
}

/// Get the current REP-2002 lifecycle state of the C++ executor's state machine.
///
/// Returns `0` (`Unconfigured`) if the executor is null or lifecycle services are
/// not registered yet. State numbering: `Unconfigured = 0`, `Inactive = 1`,
/// `Active = 2`, `Finalized = 3`.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*` produced by `nros_cpp_init`.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_get_state(executor: *mut c_void) -> u8 {
    if executor.is_null() {
        return 0;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    match ctx.executor.lifecycle_state_machine() {
        Some(sm) => sm.state() as u8,
        None => 0,
    }
}

/// Shared body for the six `nros_cpp_lifecycle_register_on_*` shims: recover the
/// `CppContext`'s executor state machine, install `cb` in `slot`, and point the
/// machine's single callback context at `ctx`.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`; `cb` and `ctx` must remain live
/// for as long as the callback can fire.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
unsafe fn register_cpp(
    executor: *mut c_void,
    slot: LifecycleCallbackSlot,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    if executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let context = unsafe { &mut *(executor as *mut CppContext) };
    let Some(sm) = context.executor.lifecycle_state_machine_mut() else {
        return NROS_CPP_RET_NOT_INIT;
    };
    sm.register(slot, Some(cb));
    sm.set_context(ctx);
    NROS_CPP_RET_OK
}

/// Register the `on_configure` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_configure(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Configure, cb, ctx) }
}

/// Register the `on_activate` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_activate(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Activate, cb, ctx) }
}

/// Register the `on_deactivate` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_deactivate(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Deactivate, cb, ctx) }
}

/// Register the `on_cleanup` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_cleanup(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Cleanup, cb, ctx) }
}

/// Register the `on_shutdown` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_shutdown(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Shutdown, cb, ctx) }
}

/// Register the `on_error` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_error(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Error, cb, ctx) }
}

/// Register lifecycle services and optionally drive the node to a higher
/// autostart state.
///
/// `autostart_code`: 0 = services only (none), 1 = configure, 2 = active.
///
/// # Safety
/// Same as [`nros_cpp_register_lifecycle_services`] and
/// [`nros_cpp_lifecycle_change_state`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_autostart(
    executor: *mut c_void,
    autostart_code: u8,
) -> nros_cpp_ret_t {
    let ret = unsafe { nros_cpp_register_lifecycle_services(executor) };
    if ret != NROS_CPP_RET_OK {
        return ret;
    }
    // autostart_code: 1 = configure, 2 = configure + activate.
    if autostart_code >= 1 {
        // Configure (transition_id = 1).
        let ret = unsafe { nros_cpp_lifecycle_change_state(executor, 1) };
        if ret != NROS_CPP_RET_OK {
            return ret;
        }
    }
    if autostart_code >= 2 {
        // Activate (transition_id = 2).
        let ret = unsafe { nros_cpp_lifecycle_change_state(executor, 2) };
        if ret != NROS_CPP_RET_OK {
            return ret;
        }
    }
    NROS_CPP_RET_OK
}

#[cfg(test)]
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
mod tests {
    use core::ptr;

    use super::*;

    unsafe extern "C" fn dummy_cb(_ctx: *mut core::ffi::c_void) -> u8 {
        0
    }

    /// Null-pointer guard: every shim fn returns INVALID_ARGUMENT for a null executor.
    #[test]
    fn null_executor_returns_invalid_argument() {
        let ret = unsafe { nros_cpp_register_lifecycle_services(ptr::null_mut()) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        let ret = unsafe { nros_cpp_lifecycle_change_state(ptr::null_mut(), 1) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        let ret = unsafe { nros_cpp_lifecycle_autostart(ptr::null_mut(), 0) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        for reg in [
            nros_cpp_lifecycle_register_on_configure as unsafe extern "C" fn(_, _, _) -> _,
            nros_cpp_lifecycle_register_on_activate,
            nros_cpp_lifecycle_register_on_deactivate,
            nros_cpp_lifecycle_register_on_cleanup,
            nros_cpp_lifecycle_register_on_shutdown,
            nros_cpp_lifecycle_register_on_error,
        ] {
            let ret = unsafe { reg(ptr::null_mut(), dummy_cb, ptr::null_mut()) };
            assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        }
    }

    /// Null executor reads back as `Unconfigured` (0) rather than trapping.
    #[test]
    fn null_executor_get_state_is_unconfigured() {
        assert_eq!(unsafe { nros_cpp_lifecycle_get_state(ptr::null_mut()) }, 0);
    }
}
