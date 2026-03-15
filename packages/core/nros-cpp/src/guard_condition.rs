//! Guard condition FFI functions for the C++ API.

use core::ffi::c_void;

use nros_node::GuardConditionHandle;

use crate::{
    CPP_GUARD_HANDLE_OPAQUE_U64S, CppContext, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_OK, nros_cpp_ret_t,
};

// Compile-time assertion: inline storage must fit GuardConditionHandle.
const _: () = assert!(
    core::mem::size_of::<GuardConditionHandle>()
        <= CPP_GUARD_HANDLE_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "CPP_GUARD_HANDLE_OPAQUE_U64S too small for GuardConditionHandle — increase the constant in lib.rs"
);

/// C callback type for guard conditions: `void callback(void* context)`.
pub type nros_cpp_guard_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Create a guard condition and register it with the executor.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `CPP_GUARD_HANDLE_OPAQUE_U64S * 8` bytes, aligned to 8 bytes.
/// The guard condition handle is written directly into this buffer.
///
/// # Safety
/// `executor_handle` and `storage` must be valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_create(
    executor_handle: *mut c_void,
    callback: nros_cpp_guard_callback_t,
    context: *mut c_void,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() || storage.is_null() {
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
            // Write directly into caller-provided storage (no heap allocation)
            unsafe {
                core::ptr::write(storage as *mut GuardConditionHandle, guard_handle);
            }
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_FULL,
    }
}

/// Trigger a guard condition (thread-safe).
///
/// # Safety
/// `storage` must be a valid guard condition storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_trigger(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let guard = unsafe { &*(storage as *const GuardConditionHandle) };
    guard.trigger();
    NROS_CPP_RET_OK
}

/// Destroy a guard condition (drop in place, no free).
///
/// # Safety
/// `storage` must be a valid initialized guard condition storage, or NULL (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }
    unsafe {
        core::ptr::drop_in_place(storage as *mut GuardConditionHandle);
    }
    NROS_CPP_RET_OK
}
