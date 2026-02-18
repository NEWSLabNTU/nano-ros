//! Lifecycle node API for the C FFI.
//!
//! Provides an rclc-compatible lifecycle state machine for C applications.

use core::ffi::c_void;

use nros_core::lifecycle::{
    LifecycleState, LifecycleTransition, TransitionResult, apply_transition, can_transition,
};

use crate::error::*;
use crate::node::nros_node_t;

// ============================================================================
// Constants
// ============================================================================

/// Lifecycle state: Unconfigured
pub const NROS_LIFECYCLE_STATE_UNCONFIGURED: u8 = LifecycleState::Unconfigured as u8;
/// Lifecycle state: Inactive
pub const NROS_LIFECYCLE_STATE_INACTIVE: u8 = LifecycleState::Inactive as u8;
/// Lifecycle state: Active
pub const NROS_LIFECYCLE_STATE_ACTIVE: u8 = LifecycleState::Active as u8;
/// Lifecycle state: Finalized
pub const NROS_LIFECYCLE_STATE_FINALIZED: u8 = LifecycleState::Finalized as u8;
/// Lifecycle state: ErrorProcessing
pub const NROS_LIFECYCLE_STATE_ERROR_PROCESSING: u8 = LifecycleState::ErrorProcessing as u8;

/// Lifecycle transition: Configure
pub const NROS_LIFECYCLE_TRANSITION_CONFIGURE: u8 = LifecycleTransition::Configure as u8;
/// Lifecycle transition: Activate
pub const NROS_LIFECYCLE_TRANSITION_ACTIVATE: u8 = LifecycleTransition::Activate as u8;
/// Lifecycle transition: Deactivate
pub const NROS_LIFECYCLE_TRANSITION_DEACTIVATE: u8 = LifecycleTransition::Deactivate as u8;
/// Lifecycle transition: Cleanup
pub const NROS_LIFECYCLE_TRANSITION_CLEANUP: u8 = LifecycleTransition::Cleanup as u8;
/// Lifecycle transition: Shutdown (from Unconfigured)
pub const NROS_LIFECYCLE_TRANSITION_SHUTDOWN_UNCONFIGURED: u8 =
    LifecycleTransition::ShutdownUnconfigured as u8;
/// Lifecycle transition: Shutdown (from Inactive)
pub const NROS_LIFECYCLE_TRANSITION_SHUTDOWN_INACTIVE: u8 =
    LifecycleTransition::ShutdownInactive as u8;
/// Lifecycle transition: Shutdown (from Active)
pub const NROS_LIFECYCLE_TRANSITION_SHUTDOWN_ACTIVE: u8 =
    LifecycleTransition::ShutdownActive as u8;
/// Lifecycle transition: Error Recovery
pub const NROS_LIFECYCLE_TRANSITION_ERROR_RECOVERY: u8 =
    LifecycleTransition::ErrorRecovery as u8;

/// Transition result: Success
pub const NROS_LIFECYCLE_RET_OK: u8 = TransitionResult::Success as u8;
/// Transition result: Failure
pub const NROS_LIFECYCLE_RET_FAILURE: u8 = TransitionResult::Failure as u8;
/// Transition result: Error
pub const NROS_LIFECYCLE_RET_ERROR: u8 = TransitionResult::Error as u8;

// ============================================================================
// Types
// ============================================================================

/// Lifecycle state machine structure.
///
/// Manages the REP-2002 lifecycle state and transition callbacks for a node.
/// Created with `nano_ros_lifecycle_get_zero_initialized()` and initialized
/// with `nano_ros_lifecycle_init()`.
#[repr(C)]
pub struct nano_ros_lifecycle_state_machine_t {
    /// Current lifecycle state (one of the `NROS_LIFECYCLE_STATE_*` constants)
    pub current_state: u8,
    /// Configure callback: called during Unconfigured -> Inactive
    pub on_configure: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    /// Activate callback: called during Inactive -> Active
    pub on_activate: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    /// Deactivate callback: called during Active -> Inactive
    pub on_deactivate: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    /// Cleanup callback: called during Inactive -> Unconfigured
    pub on_cleanup: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    /// Shutdown callback: called during any state -> Finalized
    pub on_shutdown: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    /// Error callback: called during ErrorProcessing -> Unconfigured
    pub on_error: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    /// User context pointer passed to callbacks
    pub context: *mut c_void,
    /// Whether the state machine has been initialized
    pub initialized: bool,
}

impl Default for nano_ros_lifecycle_state_machine_t {
    fn default() -> Self {
        Self {
            current_state: 0,
            on_configure: None,
            on_activate: None,
            on_deactivate: None,
            on_cleanup: None,
            on_shutdown: None,
            on_error: None,
            context: core::ptr::null_mut(),
            initialized: false,
        }
    }
}

// ============================================================================
// Functions
// ============================================================================

/// Get a zero-initialized lifecycle state machine.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_lifecycle_get_zero_initialized() -> nano_ros_lifecycle_state_machine_t {
    nano_ros_lifecycle_state_machine_t::default()
}

/// Initialize a lifecycle state machine for a node.
///
/// Sets the state to Unconfigured and marks the state machine as initialized.
///
/// # Parameters
/// * `sm` - Pointer to a zero-initialized state machine
/// * `node` - Pointer to an initialized node (must outlive the state machine)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NROS_RET_BAD_SEQUENCE` if already initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_init(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    node: *const nros_node_t,
) -> nano_ros_ret_t {
    if sm.is_null() || node.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let sm = &mut *sm;
    if sm.initialized {
        return NROS_RET_BAD_SEQUENCE;
    }

    sm.current_state = NROS_LIFECYCLE_STATE_UNCONFIGURED;
    sm.initialized = true;

    NROS_RET_OK
}

/// Finalize a lifecycle state machine.
///
/// # Parameters
/// * `sm` - Pointer to an initialized state machine
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if sm is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_fini(
    sm: *mut nano_ros_lifecycle_state_machine_t,
) -> nano_ros_ret_t {
    if sm.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let sm = &mut *sm;
    if !sm.initialized {
        return NROS_RET_NOT_INIT;
    }

    sm.current_state = NROS_LIFECYCLE_STATE_FINALIZED;
    sm.on_configure = None;
    sm.on_activate = None;
    sm.on_deactivate = None;
    sm.on_cleanup = None;
    sm.on_shutdown = None;
    sm.on_error = None;
    sm.context = core::ptr::null_mut();
    sm.initialized = false;

    NROS_RET_OK
}

/// Trigger a lifecycle state transition.
///
/// Validates the transition, invokes the registered callback (if any),
/// and applies the result per REP-2002.
///
/// # Parameters
/// * `sm` - Pointer to an initialized state machine
/// * `transition_id` - One of the `NROS_LIFECYCLE_TRANSITION_*` constants
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if sm is NULL or transition_id is invalid
/// * `NROS_RET_NOT_INIT` if not initialized
/// * `NROS_RET_BAD_SEQUENCE` if transition is not valid from current state
/// * `NROS_RET_ERROR` if the callback returned an error
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_change_state(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    transition_id: u8,
) -> nano_ros_ret_t {
    if sm.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let sm = &mut *sm;
    if !sm.initialized {
        return NROS_RET_NOT_INIT;
    }

    let Some(transition) = LifecycleTransition::from_u8(transition_id) else {
        return NROS_RET_INVALID_ARGUMENT;
    };

    let Some(state) = LifecycleState::from_u8(sm.current_state) else {
        return NROS_RET_ERROR;
    };

    if !can_transition(state, transition) {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Invoke callback
    let cb = match transition {
        LifecycleTransition::Configure => sm.on_configure,
        LifecycleTransition::Activate => sm.on_activate,
        LifecycleTransition::Deactivate => sm.on_deactivate,
        LifecycleTransition::Cleanup => sm.on_cleanup,
        LifecycleTransition::ShutdownUnconfigured
        | LifecycleTransition::ShutdownInactive
        | LifecycleTransition::ShutdownActive => sm.on_shutdown,
        LifecycleTransition::ErrorRecovery => sm.on_error,
    };

    let result = match cb {
        Some(f) => {
            let ret = f(sm.context);
            TransitionResult::from_u8(ret).unwrap_or(TransitionResult::Error)
        }
        None => TransitionResult::Success,
    };

    let new_state = apply_transition(state, transition, result);
    sm.current_state = new_state as u8;

    match result {
        TransitionResult::Success => NROS_RET_OK,
        TransitionResult::Failure => NROS_RET_ERROR,
        TransitionResult::Error => NROS_RET_ERROR,
    }
}

/// Get the current lifecycle state.
///
/// # Parameters
/// * `sm` - Pointer to an initialized state machine
///
/// # Returns
/// * Current state as u8, or 0 if sm is NULL or not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_get_state(
    sm: *const nano_ros_lifecycle_state_machine_t,
) -> u8 {
    if sm.is_null() {
        return 0;
    }

    let sm = &*sm;
    if !sm.initialized {
        return 0;
    }

    sm.current_state
}

/// Register a callback for the `configure` transition.
///
/// # Parameters
/// * `sm` - Pointer to an initialized state machine
/// * `cb` - Callback function, or NULL to clear
/// * `context` - User context passed to the callback
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if sm is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_register_on_configure(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    cb: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    context: *mut c_void,
) -> nano_ros_ret_t {
    register_callback(sm, cb, context, CallbackSlot::Configure)
}

/// Register a callback for the `activate` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_register_on_activate(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    cb: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    context: *mut c_void,
) -> nano_ros_ret_t {
    register_callback(sm, cb, context, CallbackSlot::Activate)
}

/// Register a callback for the `deactivate` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_register_on_deactivate(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    cb: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    context: *mut c_void,
) -> nano_ros_ret_t {
    register_callback(sm, cb, context, CallbackSlot::Deactivate)
}

/// Register a callback for the `cleanup` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_register_on_cleanup(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    cb: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    context: *mut c_void,
) -> nano_ros_ret_t {
    register_callback(sm, cb, context, CallbackSlot::Cleanup)
}

/// Register a callback for the `shutdown` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_register_on_shutdown(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    cb: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    context: *mut c_void,
) -> nano_ros_ret_t {
    register_callback(sm, cb, context, CallbackSlot::Shutdown)
}

/// Register a callback for the `error` transition (error recovery).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_lifecycle_register_on_error(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    cb: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    context: *mut c_void,
) -> nano_ros_ret_t {
    register_callback(sm, cb, context, CallbackSlot::Error)
}

/// Convenience: initialize a lifecycle state machine for a node.
///
/// Equivalent to calling `nano_ros_lifecycle_init(sm, node)`.
/// Named to match rclc's `rclc_make_node_a_lifecycle_node`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_make_node_a_lifecycle_node(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    node: *const nros_node_t,
) -> nano_ros_ret_t {
    nano_ros_lifecycle_init(sm, node)
}

// ============================================================================
// Internal helpers
// ============================================================================

enum CallbackSlot {
    Configure,
    Activate,
    Deactivate,
    Cleanup,
    Shutdown,
    Error,
}

unsafe fn register_callback(
    sm: *mut nano_ros_lifecycle_state_machine_t,
    cb: Option<unsafe extern "C" fn(*mut c_void) -> u8>,
    context: *mut c_void,
    slot: CallbackSlot,
) -> nano_ros_ret_t {
    if sm.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let sm = &mut *sm;
    if !sm.initialized {
        return NROS_RET_NOT_INIT;
    }

    match slot {
        CallbackSlot::Configure => sm.on_configure = cb,
        CallbackSlot::Activate => sm.on_activate = cb,
        CallbackSlot::Deactivate => sm.on_deactivate = cb,
        CallbackSlot::Cleanup => sm.on_cleanup = cb,
        CallbackSlot::Shutdown => sm.on_shutdown = cb,
        CallbackSlot::Error => sm.on_error = cb,
    }
    sm.context = context;

    NROS_RET_OK
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_initialized() {
        let sm = nano_ros_lifecycle_get_zero_initialized();
        assert_eq!(sm.current_state, 0);
        assert!(!sm.initialized);
        assert!(sm.on_configure.is_none());
        assert!(sm.context.is_null());
    }

    #[test]
    fn test_init_null_checks() {
        unsafe {
            assert_eq!(
                nano_ros_lifecycle_init(core::ptr::null_mut(), core::ptr::null()),
                NROS_RET_INVALID_ARGUMENT
            );

            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            assert_eq!(
                nano_ros_lifecycle_init(&mut sm, core::ptr::null()),
                NROS_RET_INVALID_ARGUMENT
            );
        }
    }

    #[test]
    fn test_init_and_fini() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            // Use a non-null dummy for node
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;

            assert_eq!(nano_ros_lifecycle_init(&mut sm, node_ptr), NROS_RET_OK);
            assert!(sm.initialized);
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_UNCONFIGURED);

            // Double init fails
            assert_eq!(
                nano_ros_lifecycle_init(&mut sm, node_ptr),
                NROS_RET_BAD_SEQUENCE
            );

            assert_eq!(nano_ros_lifecycle_fini(&mut sm), NROS_RET_OK);
            assert!(!sm.initialized);

            // Double fini fails
            assert_eq!(nano_ros_lifecycle_fini(&mut sm), NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_get_state() {
        unsafe {
            // NULL returns 0
            assert_eq!(nano_ros_lifecycle_get_state(core::ptr::null()), 0);

            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            // Not initialized returns 0
            assert_eq!(nano_ros_lifecycle_get_state(&sm), 0);

            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            assert_eq!(
                nano_ros_lifecycle_get_state(&sm),
                NROS_LIFECYCLE_STATE_UNCONFIGURED
            );
        }
    }

    unsafe extern "C" fn cb_success(_ctx: *mut c_void) -> u8 {
        NROS_LIFECYCLE_RET_OK
    }

    unsafe extern "C" fn cb_failure(_ctx: *mut c_void) -> u8 {
        NROS_LIFECYCLE_RET_FAILURE
    }

    unsafe extern "C" fn cb_error(_ctx: *mut c_void) -> u8 {
        NROS_LIFECYCLE_RET_ERROR
    }

    #[test]
    fn test_change_state_happy_path() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            nano_ros_lifecycle_register_on_configure(
                &mut sm,
                Some(cb_success),
                core::ptr::null_mut(),
            );

            // Configure
            assert_eq!(
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE),
                NROS_RET_OK
            );
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_INACTIVE);

            // Activate
            assert_eq!(
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_ACTIVATE),
                NROS_RET_OK
            );
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_ACTIVE);

            // Deactivate
            assert_eq!(
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_DEACTIVATE),
                NROS_RET_OK
            );
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_INACTIVE);

            // Shutdown
            assert_eq!(
                nano_ros_lifecycle_change_state(
                    &mut sm,
                    NROS_LIFECYCLE_TRANSITION_SHUTDOWN_INACTIVE
                ),
                NROS_RET_OK
            );
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_FINALIZED);
        }
    }

    #[test]
    fn test_change_state_invalid_transition() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            // Cannot activate from Unconfigured
            assert_eq!(
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_ACTIVATE),
                NROS_RET_BAD_SEQUENCE
            );
            // State unchanged
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_UNCONFIGURED);
        }
    }

    #[test]
    fn test_change_state_null_and_not_init() {
        unsafe {
            assert_eq!(
                nano_ros_lifecycle_change_state(
                    core::ptr::null_mut(),
                    NROS_LIFECYCLE_TRANSITION_CONFIGURE
                ),
                NROS_RET_INVALID_ARGUMENT
            );

            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            assert_eq!(
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE),
                NROS_RET_NOT_INIT
            );
        }
    }

    #[test]
    fn test_change_state_invalid_transition_id() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            assert_eq!(
                nano_ros_lifecycle_change_state(&mut sm, 99),
                NROS_RET_INVALID_ARGUMENT
            );
        }
    }

    #[test]
    fn test_callback_failure_rolls_back() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            nano_ros_lifecycle_register_on_configure(
                &mut sm,
                Some(cb_failure),
                core::ptr::null_mut(),
            );

            let ret =
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE);
            assert_eq!(ret, NROS_RET_ERROR);
            // State rolled back to Unconfigured
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_UNCONFIGURED);
        }
    }

    #[test]
    fn test_callback_error_goes_to_error_processing() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            nano_ros_lifecycle_register_on_configure(
                &mut sm,
                Some(cb_error),
                core::ptr::null_mut(),
            );

            let ret =
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE);
            assert_eq!(ret, NROS_RET_ERROR);
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_ERROR_PROCESSING);
        }
    }

    #[test]
    fn test_register_callback_null_and_not_init() {
        unsafe {
            assert_eq!(
                nano_ros_lifecycle_register_on_configure(
                    core::ptr::null_mut(),
                    Some(cb_success),
                    core::ptr::null_mut()
                ),
                NROS_RET_INVALID_ARGUMENT
            );

            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            assert_eq!(
                nano_ros_lifecycle_register_on_configure(
                    &mut sm,
                    Some(cb_success),
                    core::ptr::null_mut()
                ),
                NROS_RET_NOT_INIT
            );
        }
    }

    #[test]
    fn test_make_node_a_lifecycle_node() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;

            assert_eq!(
                nano_ros_make_node_a_lifecycle_node(&mut sm, node_ptr),
                NROS_RET_OK
            );
            assert!(sm.initialized);
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_UNCONFIGURED);
        }
    }

    use core::sync::atomic::{AtomicU32, Ordering};

    static CALLBACK_COUNT: AtomicU32 = AtomicU32::new(0);

    unsafe extern "C" fn cb_counting(_ctx: *mut c_void) -> u8 {
        CALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
        NROS_LIFECYCLE_RET_OK
    }

    #[test]
    fn test_callback_invocation_with_context() {
        unsafe {
            CALLBACK_COUNT.store(0, Ordering::Relaxed);

            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            nano_ros_lifecycle_register_on_configure(
                &mut sm,
                Some(cb_counting),
                core::ptr::null_mut(),
            );

            nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE);
            assert_eq!(CALLBACK_COUNT.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn test_no_callback_defaults_to_success() {
        unsafe {
            let mut sm = nano_ros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nano_ros_lifecycle_init(&mut sm, node_ptr);

            // No callbacks registered — transition should succeed
            assert_eq!(
                nano_ros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE),
                NROS_RET_OK
            );
            assert_eq!(sm.current_state, NROS_LIFECYCLE_STATE_INACTIVE);
        }
    }

    #[test]
    fn test_fini_null() {
        unsafe {
            assert_eq!(
                nano_ros_lifecycle_fini(core::ptr::null_mut()),
                NROS_RET_INVALID_ARGUMENT
            );
        }
    }
}
