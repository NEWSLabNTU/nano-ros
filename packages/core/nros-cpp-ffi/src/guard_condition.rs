//! Guard condition FFI functions for the C++ API.

use core::ffi::c_void;

use nros_node::GuardConditionHandle;

use crate::{
    CppContext, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK, nros_cpp_ret_t,
};

/// C callback type for guard conditions: `void callback(void* context)`.
pub type nros_cpp_guard_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Create a guard condition and register it with the executor.
///
/// Guard conditions allow signaling the executor from any thread.
/// The callback (if provided) is invoked during `spin_once()` when triggered.
///
/// # Parameters
/// * `executor_handle` — Executor handle from `nros_cpp_init()`.
/// * `callback` — Optional function called when triggered (may be NULL).
/// * `context` — User context passed to the callback.
/// * `out_handle` — Receives the opaque guard condition handle (for trigger/destroy).
///
/// # Safety
/// `executor_handle` and `out_handle` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_create(
    executor_handle: *mut c_void,
    callback: nros_cpp_guard_callback_t,
    context: *mut c_void,
    out_handle: *mut *mut c_void,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() || out_handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let c_callback = callback;
    let c_context = context;

    let wrapper = move || {
        if let Some(cb) = c_callback {
            unsafe {
                cb(c_context);
            }
        }
    };

    match ctx.executor.add_guard_condition(wrapper) {
        Ok((_handle_id, guard_handle)) => {
            let boxed = alloc::boxed::Box::new(guard_handle);
            unsafe {
                *out_handle = alloc::boxed::Box::into_raw(boxed) as *mut c_void;
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_FULL,
    }
}

/// Trigger a guard condition (thread-safe).
///
/// This sets the guard condition's atomic flag. The callback will be
/// invoked on the next `spin_once()` call.
///
/// # Safety
/// `handle` must be a valid guard condition handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_trigger(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let guard = unsafe { &*(handle as *const GuardConditionHandle) };
    guard.trigger();
    NROS_CPP_RET_OK
}

/// Destroy a guard condition and free its handle.
///
/// The guard condition callback entry remains in the executor arena
/// but will no longer fire (no external trigger possible after this).
///
/// # Safety
/// `handle` must be a valid guard condition handle, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_destroy(handle: *mut c_void) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        let _guard = alloc::boxed::Box::from_raw(handle as *mut GuardConditionHandle);
    }
    NROS_CPP_RET_OK
}
