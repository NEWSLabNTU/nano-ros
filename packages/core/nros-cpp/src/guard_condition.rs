//! Guard condition FFI functions for the C++ API.

use core::ffi::c_void;

use nros_node::GuardConditionHandle;

use crate::{
    CppContext, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK, nros_cpp_ret_t,
};

// Phase 87.6: no compile-time assertion needed — the C++ side sizes
// `storage_` to `NROS_GUARD_CONDITION_SIZE`, which is literally
// `size_of::<GuardConditionHandle>()` (probed from the nros rlib).

/// C callback type for guard conditions: `void callback(void* context)`.
pub type nros_cpp_guard_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Create a guard condition and register it with the executor.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<GuardConditionHandle>()` bytes (exposed via
/// `NROS_GUARD_CONDITION_SIZE`). The guard condition handle is written
/// directly into this buffer.
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

/// Relocate a `GuardConditionHandle` from `old_storage` to `new_storage`.
///
/// The handle itself contains a `&'static AtomicBool` pointing into the
/// executor arena (stable address); the wrapper closure stored in the
/// arena captures the user's `context` pointer (also stable, provided by
/// the caller), not the storage address. So relocation is a straight
/// `ptr::read` + `ptr::write`.
///
/// # Safety
/// See `nros_cpp_publisher_relocate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_relocate(
    old_storage: *mut c_void,
    new_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if old_storage.is_null() || new_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe {
        let value = core::ptr::read(old_storage as *mut GuardConditionHandle);
        core::ptr::write(new_storage as *mut GuardConditionHandle, value);
    }
    NROS_CPP_RET_OK
}
