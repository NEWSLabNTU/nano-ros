//! Guard condition FFI functions for the C++ API.
//!
//! Issue 0436 — user-supplied executor handles are tag-validated via
//! `cpp_ctx_checked` instead of being blind-cast to `*mut CppContext`.

use core::ffi::c_void;

use nros_node::GuardCondition;

use crate::{
    NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_OK, cpp_ctx_checked,
    nros_cpp_ret_t,
};

// Phase 87.6: no compile-time assertion needed — the C++ side sizes
// `storage_` to `NROS_GUARD_CONDITION_SIZE`, which is literally
// `size_of::<GuardCondition>()` (probed from the nros rlib).

/// C callback type for guard conditions: `void callback(void* context)`.
pub type nros_cpp_guard_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Create a guard condition and register it with the executor.
///
/// The caller provides `storage` — a pointer to a buffer of at least
/// `size_of::<GuardCondition>()` bytes (exposed via
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
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let c_callback = callback;
    let c_context = context;

    let wrapper = move || {
        if let Some(cb) = c_callback {
            unsafe {
                cb(c_context);
            }
        }
    };

    match ctx.executor.register_guard_condition(wrapper) {
        Ok((_handle_id, guard_handle)) => {
            // Write directly into caller-provided storage (no heap allocation)
            unsafe {
                core::ptr::write(storage as *mut GuardCondition, guard_handle);
            }
            // phase-308 — guard conditions never reach the RMW either; one
            // callback slot, same as a timer. No-op unless `metadata-mode`.
            crate::metadata_hooks::on_guard_condition_create();
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

    let guard = unsafe { &*(storage as *const GuardCondition) };
    guard.trigger();
    NROS_CPP_RET_OK
}

/// Is this guard condition triggered and not yet dispatched?
///
/// phase-417 W4.e — the RFC-0022 polling reader C has had all along, which C++
/// and Rust did not: the ledger's `c:guard_condition_is_triggered` recorded
/// "a polling C++ or Rust user cannot do what a polling C user can" as a
/// three-language disagreement, and stage 4 is where those close. The executor
/// consumes the flag on dispatch, so this answers "set and not yet dispatched".
///
/// # Safety
/// `storage` must be a valid guard condition storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_is_triggered(storage: *const c_void) -> bool {
    if storage.is_null() {
        return false;
    }
    let guard = unsafe { &*(storage as *const GuardCondition) };
    guard.is_triggered()
}

/// Clear the triggered flag without dispatching — the other half of
/// [`nros_cpp_guard_condition_is_triggered`].
///
/// # Safety
/// `storage` must be a valid guard condition storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_guard_condition_clear(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let guard = unsafe { &*(storage as *const GuardCondition) };
    let _ = guard.clear();
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
        core::ptr::drop_in_place(storage as *mut GuardCondition);
    }
    NROS_CPP_RET_OK
}

/// Relocate a `GuardCondition` from `old_storage` to `new_storage`.
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
        let value = core::ptr::read(old_storage as *mut GuardCondition);
        core::ptr::write(new_storage as *mut GuardCondition, value);
    }
    NROS_CPP_RET_OK
}
