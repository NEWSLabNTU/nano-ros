//! Guard condition API for nros C API.
//!
//! Guard conditions provide a mechanism for signaling the executor from
//! another thread. They are used for shutdown requests, custom triggers,
//! and inter-thread communication.

use core::ffi::{c_int, c_void};
use core::ptr;

use crate::error::*;
use crate::support::{nros_support_state_t, nros_support_t};

// ============================================================================
// Guard Condition Types
// ============================================================================

/// Guard condition state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_guard_condition_state_t {
    /// Not initialized
    NROS_GUARD_CONDITION_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_GUARD_CONDITION_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_GUARD_CONDITION_STATE_SHUTDOWN = 2,
}

/// Guard condition callback type.
pub type nros_guard_condition_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Guard condition structure.
#[repr(C)]
pub struct nros_guard_condition_t {
    /// Current state
    pub state: nros_guard_condition_state_t,
    /// Triggered flag (volatile for cross-thread visibility)
    pub triggered: bool,
    /// Callback function
    pub callback: nros_guard_condition_callback_t,
    /// User context pointer
    pub context: *mut c_void,
    /// Pointer to parent support context
    pub _support: *const nros_support_t,
    /// Handle ID from executor registration (usize::MAX = not registered)
    pub handle_id: usize,
    /// Guard condition handle for external triggering (set by executor)
    pub _guard_handle: *mut core::ffi::c_void,
}

// Safety: The triggered flag is designed for cross-thread access.
// The callback and context are only accessed from the executor thread.
unsafe impl Send for nros_guard_condition_t {}
unsafe impl Sync for nros_guard_condition_t {}

impl Default for nros_guard_condition_t {
    fn default() -> Self {
        Self {
            state: nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_UNINITIALIZED,
            triggered: false,
            callback: None,
            context: ptr::null_mut(),
            _support: ptr::null(),
            handle_id: usize::MAX,
            _guard_handle: ptr::null_mut(),
        }
    }
}

impl nros_guard_condition_t {
    /// Get the callback function.
    pub(crate) fn get_callback(&self) -> nros_guard_condition_callback_t {
        self.callback
    }

    /// Get the context pointer.
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }

    /// Set the handle ID from executor registration.
    pub(crate) fn set_handle_id(&mut self, id: nros_node::HandleId) {
        self.handle_id = id.0;
    }

    /// Set the guard handle for external triggering.
    #[cfg(feature = "alloc")]
    pub(crate) fn set_guard_handle(&mut self, handle: nros_node::GuardConditionHandle) {
        let boxed = alloc::boxed::Box::new(handle);
        self._guard_handle = alloc::boxed::Box::into_raw(boxed) as *mut _;
    }

    /// Get the guard handle for triggering.
    #[cfg(feature = "alloc")]
    pub(crate) fn get_guard_handle(&self) -> Option<&nros_node::GuardConditionHandle> {
        if self._guard_handle.is_null() {
            None
        } else {
            Some(unsafe { &*(self._guard_handle as *const nros_node::GuardConditionHandle) })
        }
    }
}

// ============================================================================
// Guard Condition Functions
// ============================================================================

/// Get a zero-initialized guard condition.
#[unsafe(no_mangle)]
pub extern "C" fn nros_guard_condition_get_zero_initialized() -> nros_guard_condition_t {
    nros_guard_condition_t::default()
}

/// Initialize a guard condition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_init(
    guard: *mut nros_guard_condition_t,
    support: *const nros_support_t,
) -> nros_ret_t {
    if guard.is_null() || support.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;
    let support_ref = &*support;

    // Check if already initialized
    if guard.state != nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Check if support is initialized
    if support_ref.state != nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    guard._support = support;
    guard.triggered = false;
    guard.callback = None;
    guard.context = ptr::null_mut();
    guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Set the guard condition callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_set_callback(
    guard: *mut nros_guard_condition_t,
    callback: nros_guard_condition_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    if guard.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    if guard.state != nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    guard.callback = callback;
    guard.context = context;

    NROS_RET_OK
}

/// Trigger a guard condition.
///
/// This function is designed to be thread-safe. When registered with an
/// executor, it triggers via the executor's guard handle (atomic flag in
/// the arena). Otherwise falls back to the local triggered flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_trigger(
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    if guard.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    if guard.state != nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // If registered with an executor, trigger via the executor's guard handle
    #[cfg(feature = "alloc")]
    if let Some(handle) = guard.get_guard_handle() {
        handle.trigger();
        return NROS_RET_OK;
    }

    // Fallback: use platform atomic operation for thread-safety
    crate::platform::atomic_store_bool(&mut guard.triggered as *mut bool, true);

    NROS_RET_OK
}

/// Check if the guard condition is triggered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_is_triggered(
    guard: *const nros_guard_condition_t,
) -> bool {
    if guard.is_null() {
        return false;
    }

    let guard = &*guard;

    if guard.state != nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED {
        return false;
    }

    // Use platform atomic operation for thread-safety
    crate::platform::atomic_load_bool(&guard.triggered as *const bool)
}

/// Clear the triggered flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_clear(
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    if guard.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    // Use platform atomic operation for thread-safety
    crate::platform::atomic_store_bool(&mut guard.triggered as *mut bool, false);

    NROS_RET_OK
}

/// Check if guard condition is valid (initialized).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_is_valid(
    guard: *const nros_guard_condition_t,
) -> c_int {
    if guard.is_null() {
        return 0;
    }

    let guard = &*guard;

    if guard.state == nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED {
        1
    } else {
        0
    }
}

/// Finalize a guard condition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_guard_condition_fini(
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    if guard.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    if guard.state != nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Clean up the guard handle if allocated
    #[cfg(feature = "alloc")]
    {
        if !guard._guard_handle.is_null() {
            let _handle = alloc::boxed::Box::from_raw(
                guard._guard_handle as *mut nros_node::GuardConditionHandle,
            );
        }
    }

    guard.triggered = false;
    guard.callback = None;
    guard.context = ptr::null_mut();
    guard._support = ptr::null();
    guard.handle_id = usize::MAX;
    guard._guard_handle = ptr::null_mut();
    guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_condition_default() {
        let guard = nros_guard_condition_get_zero_initialized();
        assert_eq!(
            guard.state,
            nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_UNINITIALIZED
        );
        assert!(!guard.triggered);
        assert!(guard.callback.is_none());
        assert!(guard.context.is_null());
    }

    #[test]
    fn test_guard_condition_init_null_guard() {
        unsafe {
            let support = crate::support::nros_support_get_zero_initialized();
            let ret = nros_guard_condition_init(ptr::null_mut(), &support);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_init_null_support() {
        unsafe {
            let mut guard = nros_guard_condition_get_zero_initialized();
            let ret = nros_guard_condition_init(&mut guard, ptr::null());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_trigger_not_init() {
        unsafe {
            let mut guard = nros_guard_condition_get_zero_initialized();
            let ret = nros_guard_condition_trigger(&mut guard);
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_guard_condition_trigger_null() {
        unsafe {
            let ret = nros_guard_condition_trigger(ptr::null_mut());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_is_triggered_null() {
        unsafe {
            let result = nros_guard_condition_is_triggered(ptr::null());
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_is_triggered_not_init() {
        unsafe {
            let guard = nros_guard_condition_get_zero_initialized();
            let result = nros_guard_condition_is_triggered(&guard);
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_clear_null() {
        unsafe {
            let ret = nros_guard_condition_clear(ptr::null_mut());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_is_valid_null() {
        unsafe {
            let result = nros_guard_condition_is_valid(ptr::null());
            assert_eq!(result, 0);
        }
    }

    #[test]
    fn test_guard_condition_is_valid_not_init() {
        unsafe {
            let guard = nros_guard_condition_get_zero_initialized();
            let result = nros_guard_condition_is_valid(&guard);
            assert_eq!(result, 0);
        }
    }

    #[test]
    fn test_guard_condition_fini_null() {
        unsafe {
            let ret = nros_guard_condition_fini(ptr::null_mut());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_fini_not_init() {
        unsafe {
            let mut guard = nros_guard_condition_get_zero_initialized();
            let ret = nros_guard_condition_fini(&mut guard);
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_guard_condition_set_callback_null() {
        unsafe {
            let ret = nros_guard_condition_set_callback(ptr::null_mut(), None, ptr::null_mut());
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_set_callback_not_init() {
        unsafe {
            let mut guard = nros_guard_condition_get_zero_initialized();
            let ret = nros_guard_condition_set_callback(&mut guard, None, ptr::null_mut());
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    // Test with a mock initialized guard condition
    #[test]
    fn test_guard_condition_trigger_and_clear() {
        unsafe {
            // Manually set up an initialized guard condition (bypassing support check)
            let mut guard = nros_guard_condition_get_zero_initialized();
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;

            // Initially not triggered
            assert!(!nros_guard_condition_is_triggered(&guard));

            // Trigger it
            let ret = nros_guard_condition_trigger(&mut guard);
            assert_eq!(ret, NROS_RET_OK);
            assert!(nros_guard_condition_is_triggered(&guard));

            // Clear it
            let ret = nros_guard_condition_clear(&mut guard);
            assert_eq!(ret, NROS_RET_OK);
            assert!(!nros_guard_condition_is_triggered(&guard));
        }
    }

    #[test]
    fn test_guard_condition_is_valid_initialized() {
        unsafe {
            let mut guard = nros_guard_condition_get_zero_initialized();
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;

            let result = nros_guard_condition_is_valid(&guard);
            assert_eq!(result, 1);
        }
    }

    #[test]
    fn test_guard_condition_fini_initialized() {
        unsafe {
            let mut guard = nros_guard_condition_get_zero_initialized();
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;
            guard.triggered = true;

            let ret = nros_guard_condition_fini(&mut guard);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(
                guard.state,
                nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_SHUTDOWN
            );
            assert!(!guard.triggered);
        }
    }

    // Test callback storage
    unsafe extern "C" fn test_callback(_context: *mut c_void) {}

    #[test]
    fn test_guard_condition_set_callback_initialized() {
        unsafe {
            let mut guard = nros_guard_condition_get_zero_initialized();
            guard.state = nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED;

            let context_value: i32 = 42;
            let ret = nros_guard_condition_set_callback(
                &mut guard,
                Some(test_callback),
                &context_value as *const i32 as *mut c_void,
            );
            assert_eq!(ret, NROS_RET_OK);
            assert!(guard.get_callback().is_some());
            assert!(!guard.get_context().is_null());
        }
    }
}
