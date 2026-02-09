//! Guard condition API for nano-ros C API.
//!
//! Guard conditions provide a mechanism for signaling the executor from
//! another thread. They are used for shutdown requests, custom triggers,
//! and inter-thread communication.

use core::ffi::{c_int, c_void};
use core::ptr;

use crate::error::*;
use crate::support::{nano_ros_support_state_t, nano_ros_support_t};

// ============================================================================
// Guard Condition Types
// ============================================================================

/// Guard condition state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_guard_condition_state_t {
    /// Not initialized
    NANO_ROS_GUARD_CONDITION_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED = 1,
    /// Shutdown
    NANO_ROS_GUARD_CONDITION_STATE_SHUTDOWN = 2,
}

/// Guard condition callback type.
pub type nano_ros_guard_condition_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Guard condition structure.
#[repr(C)]
pub struct nano_ros_guard_condition_t {
    /// Current state
    pub state: nano_ros_guard_condition_state_t,
    /// Triggered flag (volatile for cross-thread visibility)
    pub triggered: bool,
    /// Callback function
    callback: nano_ros_guard_condition_callback_t,
    /// User context pointer
    context: *mut c_void,
    /// Pointer to parent support context
    _support: *const nano_ros_support_t,
}

// Safety: The triggered flag is designed for cross-thread access.
// The callback and context are only accessed from the executor thread.
unsafe impl Send for nano_ros_guard_condition_t {}
unsafe impl Sync for nano_ros_guard_condition_t {}

impl Default for nano_ros_guard_condition_t {
    fn default() -> Self {
        Self {
            state: nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_UNINITIALIZED,
            triggered: false,
            callback: None,
            context: ptr::null_mut(),
            _support: ptr::null(),
        }
    }
}

impl nano_ros_guard_condition_t {
    /// Get the callback function.
    pub(crate) fn get_callback(&self) -> nano_ros_guard_condition_callback_t {
        self.callback
    }

    /// Get the context pointer.
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }
}

// ============================================================================
// Guard Condition Functions
// ============================================================================

/// Get a zero-initialized guard condition.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_guard_condition_get_zero_initialized() -> nano_ros_guard_condition_t {
    nano_ros_guard_condition_t::default()
}

/// Initialize a guard condition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_guard_condition_init(
    guard: *mut nano_ros_guard_condition_t,
    support: *const nano_ros_support_t,
) -> nano_ros_ret_t {
    if guard.is_null() || support.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;
    let support_ref = &*support;

    // Check if already initialized
    if guard.state != nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_UNINITIALIZED
    {
        return NANO_ROS_RET_BAD_SEQUENCE;
    }

    // Check if support is initialized
    if support_ref.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    guard._support = support;
    guard.triggered = false;
    guard.callback = None;
    guard.context = ptr::null_mut();
    guard.state = nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED;

    NANO_ROS_RET_OK
}

/// Set the guard condition callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_guard_condition_set_callback(
    guard: *mut nano_ros_guard_condition_t,
    callback: nano_ros_guard_condition_callback_t,
    context: *mut c_void,
) -> nano_ros_ret_t {
    if guard.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    if guard.state != nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    guard.callback = callback;
    guard.context = context;

    NANO_ROS_RET_OK
}

/// Trigger a guard condition.
///
/// This function is designed to be thread-safe. It sets the triggered flag
/// which will be checked by the executor during its next spin cycle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_guard_condition_trigger(
    guard: *mut nano_ros_guard_condition_t,
) -> nano_ros_ret_t {
    if guard.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    if guard.state != nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Use platform atomic operation for thread-safety
    crate::platform::atomic_store_bool(&mut guard.triggered as *mut bool, true);

    NANO_ROS_RET_OK
}

/// Check if the guard condition is triggered.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_guard_condition_is_triggered(
    guard: *const nano_ros_guard_condition_t,
) -> bool {
    if guard.is_null() {
        return false;
    }

    let guard = &*guard;

    if guard.state != nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED {
        return false;
    }

    // Use platform atomic operation for thread-safety
    crate::platform::atomic_load_bool(&guard.triggered as *const bool)
}

/// Clear the triggered flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_guard_condition_clear(
    guard: *mut nano_ros_guard_condition_t,
) -> nano_ros_ret_t {
    if guard.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    // Use platform atomic operation for thread-safety
    crate::platform::atomic_store_bool(&mut guard.triggered as *mut bool, false);

    NANO_ROS_RET_OK
}

/// Check if guard condition is valid (initialized).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_guard_condition_is_valid(
    guard: *const nano_ros_guard_condition_t,
) -> c_int {
    if guard.is_null() {
        return 0;
    }

    let guard = &*guard;

    if guard.state == nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED {
        1
    } else {
        0
    }
}

/// Finalize a guard condition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_guard_condition_fini(
    guard: *mut nano_ros_guard_condition_t,
) -> nano_ros_ret_t {
    if guard.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let guard = &mut *guard;

    if guard.state != nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    guard.triggered = false;
    guard.callback = None;
    guard.context = ptr::null_mut();
    guard._support = ptr::null();
    guard.state = nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_condition_default() {
        let guard = nano_ros_guard_condition_get_zero_initialized();
        assert_eq!(
            guard.state,
            nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_UNINITIALIZED
        );
        assert!(!guard.triggered);
        assert!(guard.callback.is_none());
        assert!(guard.context.is_null());
    }

    #[test]
    fn test_guard_condition_init_null_guard() {
        unsafe {
            let support = crate::support::nano_ros_support_get_zero_initialized();
            let ret = nano_ros_guard_condition_init(ptr::null_mut(), &support);
            assert_eq!(ret, NANO_ROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_init_null_support() {
        unsafe {
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            let ret = nano_ros_guard_condition_init(&mut guard, ptr::null());
            assert_eq!(ret, NANO_ROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_trigger_not_init() {
        unsafe {
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            let ret = nano_ros_guard_condition_trigger(&mut guard);
            assert_eq!(ret, NANO_ROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_guard_condition_trigger_null() {
        unsafe {
            let ret = nano_ros_guard_condition_trigger(ptr::null_mut());
            assert_eq!(ret, NANO_ROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_is_triggered_null() {
        unsafe {
            let result = nano_ros_guard_condition_is_triggered(ptr::null());
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_is_triggered_not_init() {
        unsafe {
            let guard = nano_ros_guard_condition_get_zero_initialized();
            let result = nano_ros_guard_condition_is_triggered(&guard);
            assert!(!result);
        }
    }

    #[test]
    fn test_guard_condition_clear_null() {
        unsafe {
            let ret = nano_ros_guard_condition_clear(ptr::null_mut());
            assert_eq!(ret, NANO_ROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_is_valid_null() {
        unsafe {
            let result = nano_ros_guard_condition_is_valid(ptr::null());
            assert_eq!(result, 0);
        }
    }

    #[test]
    fn test_guard_condition_is_valid_not_init() {
        unsafe {
            let guard = nano_ros_guard_condition_get_zero_initialized();
            let result = nano_ros_guard_condition_is_valid(&guard);
            assert_eq!(result, 0);
        }
    }

    #[test]
    fn test_guard_condition_fini_null() {
        unsafe {
            let ret = nano_ros_guard_condition_fini(ptr::null_mut());
            assert_eq!(ret, NANO_ROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_fini_not_init() {
        unsafe {
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            let ret = nano_ros_guard_condition_fini(&mut guard);
            assert_eq!(ret, NANO_ROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_guard_condition_set_callback_null() {
        unsafe {
            let ret = nano_ros_guard_condition_set_callback(ptr::null_mut(), None, ptr::null_mut());
            assert_eq!(ret, NANO_ROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_guard_condition_set_callback_not_init() {
        unsafe {
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            let ret = nano_ros_guard_condition_set_callback(&mut guard, None, ptr::null_mut());
            assert_eq!(ret, NANO_ROS_RET_NOT_INIT);
        }
    }

    // Test with a mock initialized guard condition
    #[test]
    fn test_guard_condition_trigger_and_clear() {
        unsafe {
            // Manually set up an initialized guard condition (bypassing support check)
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            guard.state =
                nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED;

            // Initially not triggered
            assert!(!nano_ros_guard_condition_is_triggered(&guard));

            // Trigger it
            let ret = nano_ros_guard_condition_trigger(&mut guard);
            assert_eq!(ret, NANO_ROS_RET_OK);
            assert!(nano_ros_guard_condition_is_triggered(&guard));

            // Clear it
            let ret = nano_ros_guard_condition_clear(&mut guard);
            assert_eq!(ret, NANO_ROS_RET_OK);
            assert!(!nano_ros_guard_condition_is_triggered(&guard));
        }
    }

    #[test]
    fn test_guard_condition_is_valid_initialized() {
        unsafe {
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            guard.state =
                nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED;

            let result = nano_ros_guard_condition_is_valid(&guard);
            assert_eq!(result, 1);
        }
    }

    #[test]
    fn test_guard_condition_fini_initialized() {
        unsafe {
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            guard.state =
                nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED;
            guard.triggered = true;

            let ret = nano_ros_guard_condition_fini(&mut guard);
            assert_eq!(ret, NANO_ROS_RET_OK);
            assert_eq!(
                guard.state,
                nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_SHUTDOWN
            );
            assert!(!guard.triggered);
        }
    }

    // Test callback storage
    unsafe extern "C" fn test_callback(_context: *mut c_void) {}

    #[test]
    fn test_guard_condition_set_callback_initialized() {
        unsafe {
            let mut guard = nano_ros_guard_condition_get_zero_initialized();
            guard.state =
                nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED;

            let context_value: i32 = 42;
            let ret = nano_ros_guard_condition_set_callback(
                &mut guard,
                Some(test_callback),
                &context_value as *const i32 as *mut c_void,
            );
            assert_eq!(ret, NANO_ROS_RET_OK);
            assert!(guard.get_callback().is_some());
            assert!(!guard.get_context().is_null());
        }
    }
}
