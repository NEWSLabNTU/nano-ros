//! Lifecycle node API for the C FFI.
//!
//! Thin wrapper around [`nros_node::lifecycle::LifecyclePollingNodeCtx`]. The
//! `#[repr(C)]` struct holds an opaque u64 array sized to fit the real Rust
//! state machine; all transition / callback logic lives in `nros-node` and is
//! tested there.

use core::ffi::c_void;
use core::mem::MaybeUninit;

use nros_core::lifecycle::{LifecycleState, LifecycleTransition, TransitionResult};
use nros_node::lifecycle::{
    LifecycleCallbackFnCtx, LifecycleCallbackSlot, LifecycleError, LifecyclePollingNodeCtx,
};

use crate::constants::NROS_LIFECYCLE_CTX_OPAQUE_U64S;
use crate::error::*;
use crate::node::nros_node_t;

// ============================================================================
// Constants — exported to C via cbindgen
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
pub const NROS_LIFECYCLE_TRANSITION_SHUTDOWN_ACTIVE: u8 = LifecycleTransition::ShutdownActive as u8;
/// Lifecycle transition: Error Recovery
pub const NROS_LIFECYCLE_TRANSITION_ERROR_RECOVERY: u8 = LifecycleTransition::ErrorRecovery as u8;

/// Transition result: Success
pub const NROS_LIFECYCLE_RET_OK: u8 = TransitionResult::Success as u8;
/// Transition result: Failure
pub const NROS_LIFECYCLE_RET_FAILURE: u8 = TransitionResult::Failure as u8;
/// Transition result: Error
pub const NROS_LIFECYCLE_RET_ERROR: u8 = TransitionResult::Error as u8;

// ============================================================================
// Types
// ============================================================================

/// Opaque lifecycle state machine storage.
///
/// The `storage` field holds a [`LifecyclePollingNodeCtx`] placed into a
/// `u64` array to keep `#[repr(C)]` layout predictable for C callers.
/// Treat the struct as opaque — use [`nros_lifecycle_get_state`] and the
/// `nros_lifecycle_register_on_*` functions to interact with it.
#[repr(C)]
pub struct nros_lifecycle_state_machine_t {
    /// Whether the state machine has been initialised.
    pub initialized: bool,
    /// Padding for 8-byte alignment of `storage`.
    _pad: [u8; 7],
    /// Opaque storage for the underlying Rust state machine.
    storage: [u64; NROS_LIFECYCLE_CTX_OPAQUE_U64S],
}

impl Default for nros_lifecycle_state_machine_t {
    fn default() -> Self {
        Self {
            initialized: false,
            _pad: [0; 7],
            storage: [0; NROS_LIFECYCLE_CTX_OPAQUE_U64S],
        }
    }
}

// ============================================================================
// Opaque-storage access helpers
// ============================================================================

#[inline]
unsafe fn inner_mut(
    sm: *mut nros_lifecycle_state_machine_t,
) -> &'static mut LifecyclePollingNodeCtx {
    let ptr = (*sm).storage.as_mut_ptr() as *mut LifecyclePollingNodeCtx;
    &mut *ptr
}

#[inline]
unsafe fn inner_ref(sm: *const nros_lifecycle_state_machine_t) -> &'static LifecyclePollingNodeCtx {
    let ptr = (*sm).storage.as_ptr() as *const LifecyclePollingNodeCtx;
    &*ptr
}

// ============================================================================
// Functions
// ============================================================================

/// Get a zero-initialized lifecycle state machine.
#[unsafe(no_mangle)]
pub extern "C" fn nros_lifecycle_get_zero_initialized() -> nros_lifecycle_state_machine_t {
    nros_lifecycle_state_machine_t::default()
}

/// Initialize a lifecycle state machine for a node.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_init(
    sm: *mut nros_lifecycle_state_machine_t,
    node: *const nros_node_t,
) -> nros_ret_t {
    if sm.is_null() || node.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }
    if (*sm).initialized {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Place a fresh LifecyclePollingNodeCtx into the opaque storage.
    let slot = (*sm).storage.as_mut_ptr() as *mut MaybeUninit<LifecyclePollingNodeCtx>;
    (*slot).write(LifecyclePollingNodeCtx::new());
    (*sm).initialized = true;

    NROS_RET_OK
}

/// Finalize a lifecycle state machine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_fini(
    sm: *mut nros_lifecycle_state_machine_t,
) -> nros_ret_t {
    if sm.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }
    if !(*sm).initialized {
        return NROS_RET_NOT_INIT;
    }

    let inner = inner_mut(sm);
    inner.finalize();
    inner.clear_callbacks();
    (*sm).initialized = false;

    NROS_RET_OK
}

/// Trigger a lifecycle state transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_change_state(
    sm: *mut nros_lifecycle_state_machine_t,
    transition_id: u8,
) -> nros_ret_t {
    if sm.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }
    if !(*sm).initialized {
        return NROS_RET_NOT_INIT;
    }
    let Some(transition) = LifecycleTransition::from_u8(transition_id) else {
        return NROS_RET_INVALID_ARGUMENT;
    };

    match inner_mut(sm).trigger_transition(transition) {
        Ok(_) => NROS_RET_OK,
        Err(LifecycleError::InvalidTransition { .. }) => NROS_RET_BAD_SEQUENCE,
        Err(LifecycleError::CallbackFailed { .. }) => NROS_RET_ERROR,
        Err(LifecycleError::NodeFinalized) => NROS_RET_BAD_SEQUENCE,
    }
}

/// Get the current lifecycle state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_get_state(sm: *const nros_lifecycle_state_machine_t) -> u8 {
    if sm.is_null() || !(*sm).initialized {
        return 0;
    }
    inner_ref(sm).state() as u8
}

/// Register a callback for the `configure` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_register_on_configure(
    sm: *mut nros_lifecycle_state_machine_t,
    cb: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
) -> nros_ret_t {
    register(sm, LifecycleCallbackSlot::Configure, cb, context)
}

/// Register a callback for the `activate` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_register_on_activate(
    sm: *mut nros_lifecycle_state_machine_t,
    cb: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
) -> nros_ret_t {
    register(sm, LifecycleCallbackSlot::Activate, cb, context)
}

/// Register a callback for the `deactivate` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_register_on_deactivate(
    sm: *mut nros_lifecycle_state_machine_t,
    cb: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
) -> nros_ret_t {
    register(sm, LifecycleCallbackSlot::Deactivate, cb, context)
}

/// Register a callback for the `cleanup` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_register_on_cleanup(
    sm: *mut nros_lifecycle_state_machine_t,
    cb: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
) -> nros_ret_t {
    register(sm, LifecycleCallbackSlot::Cleanup, cb, context)
}

/// Register a callback for the `shutdown` transition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_register_on_shutdown(
    sm: *mut nros_lifecycle_state_machine_t,
    cb: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
) -> nros_ret_t {
    register(sm, LifecycleCallbackSlot::Shutdown, cb, context)
}

/// Register a callback for the `error` transition (error recovery).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_lifecycle_register_on_error(
    sm: *mut nros_lifecycle_state_machine_t,
    cb: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
) -> nros_ret_t {
    register(sm, LifecycleCallbackSlot::Error, cb, context)
}

/// Convenience: alias for `nros_lifecycle_init` matching rclc's
/// `rclc_make_node_a_lifecycle_node`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_make_node_a_lifecycle_node(
    sm: *mut nros_lifecycle_state_machine_t,
    node: *const nros_node_t,
) -> nros_ret_t {
    nros_lifecycle_init(sm, node)
}

// ============================================================================
// Internal helper
// ============================================================================

#[inline]
unsafe fn register(
    sm: *mut nros_lifecycle_state_machine_t,
    slot: LifecycleCallbackSlot,
    cb: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
) -> nros_ret_t {
    if sm.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }
    if !(*sm).initialized {
        return NROS_RET_NOT_INIT;
    }
    let inner = inner_mut(sm);
    inner.register(slot, cb);
    inner.set_context(context);
    NROS_RET_OK
}

// ============================================================================
// Executor-integrated lifecycle services (ROS 2 tooling surface)
// ============================================================================
//
// These functions are gated on `lifecycle-services` + an active RMW backend
// because the Executor type is only defined when an RMW is compiled in.
// They expose the state machine owned *by the Executor* (created during
// `nros_executor_register_lifecycle_services`) — distinct from the
// standalone `nros_lifecycle_state_machine_t` used by drivers that don't
// want ROS 2 tooling integration.

#[cfg(all(
    feature = "lifecycle-services",
    any(feature = "rmw-zenoh", feature = "rmw-xrce")
))]
mod service_backed {
    use super::*;
    use crate::executor::{get_executor, nros_executor_t};

    /// Register the five REP-2002 lifecycle services on the executor's node.
    ///
    /// After this call, `ros2 lifecycle set|get|list|nodes` can drive the
    /// executor-owned state machine. Register transition callbacks via
    /// `nros_executor_lifecycle_register_on_*` and inspect the state via
    /// `nros_executor_lifecycle_get_state`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_register_lifecycle_services(
        executor: *mut nros_executor_t,
    ) -> nros_ret_t {
        if executor.is_null() {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let exec = unsafe { get_executor(&mut (*executor)._opaque) };
        match exec.register_lifecycle_services() {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_ERROR,
        }
    }

    /// Get the current lifecycle state of the executor's state machine.
    ///
    /// Returns `NROS_LIFECYCLE_STATE_UNCONFIGURED` if services are not
    /// registered yet.
    ///
    /// Takes `*mut` rather than `*const` because `get_executor` returns
    /// `&mut CExecutor` — reading the state is logically read-only but the
    /// executor accessor shares storage with the services loop that needs
    /// `&mut` during spin.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_get_state(
        executor: *mut nros_executor_t,
    ) -> u8 {
        if executor.is_null() {
            return NROS_LIFECYCLE_STATE_UNCONFIGURED;
        }
        let exec = unsafe { get_executor(&mut (*executor)._opaque) };
        match exec.lifecycle_state_machine() {
            Some(sm) => sm.state() as u8,
            None => NROS_LIFECYCLE_STATE_UNCONFIGURED,
        }
    }

    /// Trigger a lifecycle transition on the executor's state machine.
    ///
    /// # Safety
    /// Invokes the user's registered C callback through a raw function
    /// pointer. The caller must ensure the callback and any context it
    /// captures are live.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_change_state(
        executor: *mut nros_executor_t,
        transition_id: u8,
    ) -> nros_ret_t {
        if executor.is_null() {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let exec = unsafe { get_executor(&mut (*executor)._opaque) };
        let Some(sm) = exec.lifecycle_state_machine_mut() else {
            return NROS_RET_NOT_INIT;
        };
        let Some(t) = LifecycleTransition::from_u8(transition_id) else {
            return NROS_RET_INVALID_ARGUMENT;
        };
        // SAFETY: forwarded through this function's unsafe contract.
        match unsafe { sm.trigger_transition(t) } {
            Ok(_) => NROS_RET_OK,
            Err(LifecycleError::NodeFinalized) => NROS_RET_BAD_SEQUENCE,
            Err(LifecycleError::InvalidTransition { .. }) => NROS_RET_INVALID_ARGUMENT,
            Err(LifecycleError::CallbackFailed { .. }) => NROS_RET_ERROR,
        }
    }

    #[inline]
    unsafe fn register_exec(
        executor: *mut nros_executor_t,
        slot: LifecycleCallbackSlot,
        cb: Option<LifecycleCallbackFnCtx>,
        context: *mut c_void,
    ) -> nros_ret_t {
        if executor.is_null() {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let exec = unsafe { get_executor(&mut (*executor)._opaque) };
        let Some(sm) = exec.lifecycle_state_machine_mut() else {
            return NROS_RET_NOT_INIT;
        };
        sm.register(slot, cb);
        sm.set_context(context);
        NROS_RET_OK
    }

    /// Register the on-configure callback on the executor's state machine.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_register_on_configure(
        executor: *mut nros_executor_t,
        cb: Option<LifecycleCallbackFnCtx>,
        context: *mut c_void,
    ) -> nros_ret_t {
        unsafe { register_exec(executor, LifecycleCallbackSlot::Configure, cb, context) }
    }

    /// Register the on-activate callback on the executor's state machine.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_register_on_activate(
        executor: *mut nros_executor_t,
        cb: Option<LifecycleCallbackFnCtx>,
        context: *mut c_void,
    ) -> nros_ret_t {
        unsafe { register_exec(executor, LifecycleCallbackSlot::Activate, cb, context) }
    }

    /// Register the on-deactivate callback on the executor's state machine.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_register_on_deactivate(
        executor: *mut nros_executor_t,
        cb: Option<LifecycleCallbackFnCtx>,
        context: *mut c_void,
    ) -> nros_ret_t {
        unsafe { register_exec(executor, LifecycleCallbackSlot::Deactivate, cb, context) }
    }

    /// Register the on-cleanup callback on the executor's state machine.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_register_on_cleanup(
        executor: *mut nros_executor_t,
        cb: Option<LifecycleCallbackFnCtx>,
        context: *mut c_void,
    ) -> nros_ret_t {
        unsafe { register_exec(executor, LifecycleCallbackSlot::Cleanup, cb, context) }
    }

    /// Register the on-shutdown callback on the executor's state machine.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_register_on_shutdown(
        executor: *mut nros_executor_t,
        cb: Option<LifecycleCallbackFnCtx>,
        context: *mut c_void,
    ) -> nros_ret_t {
        unsafe { register_exec(executor, LifecycleCallbackSlot::Shutdown, cb, context) }
    }

    /// Register the on-error callback on the executor's state machine.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_lifecycle_register_on_error(
        executor: *mut nros_executor_t,
        cb: Option<LifecycleCallbackFnCtx>,
        context: *mut c_void,
    ) -> nros_ret_t {
        unsafe { register_exec(executor, LifecycleCallbackSlot::Error, cb, context) }
    }
}

// ============================================================================
// Tests — focused on the FFI bridge; the state machine itself is tested in
// `nros_node::lifecycle::tests`.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_initialized_and_init_fini() {
        unsafe {
            let mut sm = nros_lifecycle_get_zero_initialized();
            assert!(!sm.initialized);

            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            assert_eq!(nros_lifecycle_init(&mut sm, node_ptr), NROS_RET_OK);
            assert!(sm.initialized);
            assert_eq!(
                nros_lifecycle_get_state(&sm),
                NROS_LIFECYCLE_STATE_UNCONFIGURED
            );

            // Double-init rejected
            assert_eq!(
                nros_lifecycle_init(&mut sm, node_ptr),
                NROS_RET_BAD_SEQUENCE
            );

            assert_eq!(nros_lifecycle_fini(&mut sm), NROS_RET_OK);
            assert!(!sm.initialized);
            assert_eq!(nros_lifecycle_fini(&mut sm), NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_null_checks() {
        unsafe {
            assert_eq!(
                nros_lifecycle_init(core::ptr::null_mut(), core::ptr::null()),
                NROS_RET_INVALID_ARGUMENT
            );
            let mut sm = nros_lifecycle_get_zero_initialized();
            assert_eq!(
                nros_lifecycle_init(&mut sm, core::ptr::null()),
                NROS_RET_INVALID_ARGUMENT
            );
            assert_eq!(
                nros_lifecycle_change_state(
                    core::ptr::null_mut(),
                    NROS_LIFECYCLE_TRANSITION_CONFIGURE
                ),
                NROS_RET_INVALID_ARGUMENT
            );
            assert_eq!(nros_lifecycle_get_state(core::ptr::null()), 0);
        }
    }

    unsafe extern "C" fn cb_success(_ctx: *mut c_void) -> u8 {
        NROS_LIFECYCLE_RET_OK
    }

    unsafe extern "C" fn cb_failure(_ctx: *mut c_void) -> u8 {
        NROS_LIFECYCLE_RET_FAILURE
    }

    #[test]
    fn test_happy_path_through_ffi() {
        unsafe {
            let mut sm = nros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nros_lifecycle_init(&mut sm, node_ptr);

            nros_lifecycle_register_on_configure(&mut sm, Some(cb_success), core::ptr::null_mut());
            nros_lifecycle_register_on_activate(&mut sm, Some(cb_success), core::ptr::null_mut());

            assert_eq!(
                nros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE),
                NROS_RET_OK
            );
            assert_eq!(nros_lifecycle_get_state(&sm), NROS_LIFECYCLE_STATE_INACTIVE);
            assert_eq!(
                nros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_ACTIVATE),
                NROS_RET_OK
            );
            assert_eq!(nros_lifecycle_get_state(&sm), NROS_LIFECYCLE_STATE_ACTIVE);
        }
    }

    #[test]
    fn test_callback_failure_rolls_back() {
        unsafe {
            let mut sm = nros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nros_lifecycle_init(&mut sm, node_ptr);

            nros_lifecycle_register_on_configure(&mut sm, Some(cb_failure), core::ptr::null_mut());
            assert_eq!(
                nros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE),
                NROS_RET_ERROR
            );
            assert_eq!(
                nros_lifecycle_get_state(&sm),
                NROS_LIFECYCLE_STATE_UNCONFIGURED
            );
        }
    }

    #[test]
    fn test_invalid_transition_id() {
        unsafe {
            let mut sm = nros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nros_lifecycle_init(&mut sm, node_ptr);

            assert_eq!(
                nros_lifecycle_change_state(&mut sm, 99),
                NROS_RET_INVALID_ARGUMENT
            );
        }
    }

    #[test]
    fn test_context_passed_through() {
        use core::sync::atomic::{AtomicUsize, Ordering};
        static SEEN: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn cb(ctx: *mut c_void) -> u8 {
            SEEN.store(ctx as usize, Ordering::Relaxed);
            NROS_LIFECYCLE_RET_OK
        }

        unsafe {
            let mut sm = nros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            nros_lifecycle_init(&mut sm, node_ptr);

            nros_lifecycle_register_on_configure(&mut sm, Some(cb), 0xDEAD as *mut c_void);
            nros_lifecycle_change_state(&mut sm, NROS_LIFECYCLE_TRANSITION_CONFIGURE);
            assert_eq!(SEEN.load(Ordering::Relaxed), 0xDEAD);
        }
    }

    #[test]
    fn test_make_node_a_lifecycle_node_alias() {
        unsafe {
            let mut sm = nros_lifecycle_get_zero_initialized();
            let dummy_node = 1u8;
            let node_ptr = &dummy_node as *const u8 as *const nros_node_t;
            assert_eq!(
                nros_make_node_a_lifecycle_node(&mut sm, node_ptr),
                NROS_RET_OK
            );
            assert!(sm.initialized);
        }
    }
}
