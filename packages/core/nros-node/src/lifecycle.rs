//! Lifecycle node API (REP-2002)
//!
//! Provides managed lifecycle state machines for nros nodes.
//!
//! - [`LifecyclePollingNode`] — standalone state machine with plain function pointers (`no_std`)
//! - [`LifecyclePollingNodeCtx`] — standalone state machine with `unsafe fn(*mut c_void) -> TransitionResult`
//!   callbacks, for bridging the C FFI (`no_std`)

use core::ffi::c_void;
use nros_core::lifecycle::{
    LifecycleState, LifecycleTransition, TransitionResult, apply_transition, can_transition,
};

/// Error type for lifecycle transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    /// The requested transition is not valid from the current state.
    InvalidTransition {
        /// The state the node was in when the transition was attempted.
        from: LifecycleState,
        /// The transition that was requested.
        transition: LifecycleTransition,
    },
    /// The transition callback returned a non-success result.
    CallbackFailed {
        /// The transition that was attempted.
        transition: LifecycleTransition,
        /// The result returned by the callback.
        result: TransitionResult,
    },
    /// The node is in the Finalized state and cannot transition.
    NodeFinalized,
}

// ═══════════════════════════════════════════════════════════════════════════
// LIFECYCLE POLLING NODE (no_std — function pointers, no NodeHandle)
// ═══════════════════════════════════════════════════════════════════════════

/// Lifecycle callback function pointer (`no_std` compatible).
pub type LifecycleCallbackFn = fn() -> TransitionResult;

/// Standalone lifecycle state machine for `no_std` environments.
///
/// Uses function pointers instead of boxed closures. Does not wrap a
/// `NodeHandle` — the user manages the node separately.
///
/// # Example
///
/// ```ignore
/// fn on_configure() -> TransitionResult {
///     // Initialize hardware...
///     TransitionResult::Success
/// }
///
/// let mut lifecycle = LifecyclePollingNode::new();
/// lifecycle.register_on_configure(on_configure);
/// lifecycle.configure()?;
/// ```
pub struct LifecyclePollingNode {
    state: LifecycleState,
    on_configure: Option<LifecycleCallbackFn>,
    on_activate: Option<LifecycleCallbackFn>,
    on_deactivate: Option<LifecycleCallbackFn>,
    on_cleanup: Option<LifecycleCallbackFn>,
    on_shutdown: Option<LifecycleCallbackFn>,
    on_error: Option<LifecycleCallbackFn>,
}

impl LifecyclePollingNode {
    /// Create a new standalone lifecycle state machine.
    ///
    /// Starts in the `Unconfigured` state.
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::Unconfigured,
            on_configure: None,
            on_activate: None,
            on_deactivate: None,
            on_cleanup: None,
            on_shutdown: None,
            on_error: None,
        }
    }

    /// Get the current lifecycle state.
    pub const fn state(&self) -> LifecycleState {
        self.state
    }

    /// Trigger a lifecycle transition.
    pub fn trigger_transition(
        &mut self,
        transition: LifecycleTransition,
    ) -> Result<LifecycleState, LifecycleError> {
        if self.state.is_terminal() {
            return Err(LifecycleError::NodeFinalized);
        }

        if !can_transition(self.state, transition) {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                transition,
            });
        }

        let result = self.invoke_callback(transition);
        self.state = apply_transition(self.state, transition, result);

        if result == TransitionResult::Success {
            Ok(self.state)
        } else {
            Err(LifecycleError::CallbackFailed { transition, result })
        }
    }

    /// Convenience: configure (Unconfigured -> Inactive)
    pub fn configure(&mut self) -> Result<LifecycleState, LifecycleError> {
        self.trigger_transition(LifecycleTransition::Configure)
    }

    /// Convenience: activate (Inactive -> Active)
    pub fn activate(&mut self) -> Result<LifecycleState, LifecycleError> {
        self.trigger_transition(LifecycleTransition::Activate)
    }

    /// Convenience: deactivate (Active -> Inactive)
    pub fn deactivate(&mut self) -> Result<LifecycleState, LifecycleError> {
        self.trigger_transition(LifecycleTransition::Deactivate)
    }

    /// Convenience: cleanup (Inactive -> Unconfigured)
    pub fn cleanup(&mut self) -> Result<LifecycleState, LifecycleError> {
        self.trigger_transition(LifecycleTransition::Cleanup)
    }

    /// Convenience: shutdown from the current state.
    pub fn shutdown(&mut self) -> Result<LifecycleState, LifecycleError> {
        let transition = match self.state {
            LifecycleState::Unconfigured => LifecycleTransition::ShutdownUnconfigured,
            LifecycleState::Inactive => LifecycleTransition::ShutdownInactive,
            LifecycleState::Active => LifecycleTransition::ShutdownActive,
            LifecycleState::Finalized => return Err(LifecycleError::NodeFinalized),
            LifecycleState::ErrorProcessing => {
                return Err(LifecycleError::InvalidTransition {
                    from: self.state,
                    transition: LifecycleTransition::ShutdownUnconfigured,
                });
            }
        };
        self.trigger_transition(transition)
    }

    /// Convenience: configure then activate (stops on failure).
    pub fn bring_up(&mut self) -> Result<LifecycleState, LifecycleError> {
        self.configure()?;
        self.activate()
    }

    /// Register a callback for the `configure` transition.
    pub fn register_on_configure(&mut self, cb: LifecycleCallbackFn) {
        self.on_configure = Some(cb);
    }

    /// Register a callback for the `activate` transition.
    pub fn register_on_activate(&mut self, cb: LifecycleCallbackFn) {
        self.on_activate = Some(cb);
    }

    /// Register a callback for the `deactivate` transition.
    pub fn register_on_deactivate(&mut self, cb: LifecycleCallbackFn) {
        self.on_deactivate = Some(cb);
    }

    /// Register a callback for the `cleanup` transition.
    pub fn register_on_cleanup(&mut self, cb: LifecycleCallbackFn) {
        self.on_cleanup = Some(cb);
    }

    /// Register a callback for the `shutdown` transition.
    pub fn register_on_shutdown(&mut self, cb: LifecycleCallbackFn) {
        self.on_shutdown = Some(cb);
    }

    /// Register a callback for the `error` transition (error recovery).
    pub fn register_on_error(&mut self, cb: LifecycleCallbackFn) {
        self.on_error = Some(cb);
    }

    fn invoke_callback(&mut self, transition: LifecycleTransition) -> TransitionResult {
        let cb = match transition {
            LifecycleTransition::Configure => self.on_configure,
            LifecycleTransition::Activate => self.on_activate,
            LifecycleTransition::Deactivate => self.on_deactivate,
            LifecycleTransition::Cleanup => self.on_cleanup,
            LifecycleTransition::ShutdownUnconfigured
            | LifecycleTransition::ShutdownInactive
            | LifecycleTransition::ShutdownActive => self.on_shutdown,
            LifecycleTransition::ErrorRecovery => self.on_error,
        };

        match cb {
            Some(f) => f(),
            None => TransitionResult::Success,
        }
    }
}

impl Default for LifecyclePollingNode {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LIFECYCLE POLLING NODE WITH CONTEXT (no_std — C FFI compatible)
// ═══════════════════════════════════════════════════════════════════════════

/// Lifecycle callback taking a user context pointer (`no_std`, C FFI shape).
///
/// Returns a `u8` matching the C `NROS_LIFECYCLE_RET_*` constants
/// (`0 = Success`, `1 = Failure`, `2 = Error`). Any unknown value is
/// coerced to [`TransitionResult::Error`] inside
/// [`LifecyclePollingNodeCtx::trigger_transition`].
pub type LifecycleCallbackFnCtx = unsafe extern "C" fn(ctx: *mut c_void) -> u8;

/// Lifecycle state machine with `unsafe fn(*mut c_void) -> TransitionResult` callbacks.
///
/// Thin counterpart to [`LifecyclePollingNode`] for bridging the C FFI: each
/// callback slot stores a pointer to `extern "C"` user code plus a single
/// shared `*mut c_void` context that is passed on every invocation. The core
/// state machine logic comes from [`nros_core::lifecycle`], same as
/// `LifecyclePollingNode`.
pub struct LifecyclePollingNodeCtx {
    state: LifecycleState,
    on_configure: Option<LifecycleCallbackFnCtx>,
    on_activate: Option<LifecycleCallbackFnCtx>,
    on_deactivate: Option<LifecycleCallbackFnCtx>,
    on_cleanup: Option<LifecycleCallbackFnCtx>,
    on_shutdown: Option<LifecycleCallbackFnCtx>,
    on_error: Option<LifecycleCallbackFnCtx>,
    context: *mut c_void,
}

// `*mut c_void` is `!Sync` + `!Send`; that's the correct posture for a
// state machine owned by one task. No auto-impl needed.

impl LifecyclePollingNodeCtx {
    /// Create a new standalone lifecycle state machine. Starts in `Unconfigured`.
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::Unconfigured,
            on_configure: None,
            on_activate: None,
            on_deactivate: None,
            on_cleanup: None,
            on_shutdown: None,
            on_error: None,
            context: core::ptr::null_mut(),
        }
    }

    /// Get the current lifecycle state.
    pub const fn state(&self) -> LifecycleState {
        self.state
    }

    /// Set the user context pointer passed to every callback.
    pub fn set_context(&mut self, ctx: *mut c_void) {
        self.context = ctx;
    }

    /// Get the user context pointer.
    pub fn context(&self) -> *mut c_void {
        self.context
    }

    /// Register / clear the callback for a given transition slot.
    pub fn register(&mut self, slot: LifecycleCallbackSlot, cb: Option<LifecycleCallbackFnCtx>) {
        match slot {
            LifecycleCallbackSlot::Configure => self.on_configure = cb,
            LifecycleCallbackSlot::Activate => self.on_activate = cb,
            LifecycleCallbackSlot::Deactivate => self.on_deactivate = cb,
            LifecycleCallbackSlot::Cleanup => self.on_cleanup = cb,
            LifecycleCallbackSlot::Shutdown => self.on_shutdown = cb,
            LifecycleCallbackSlot::Error => self.on_error = cb,
        }
    }

    /// Clear every registered callback. Used on fini.
    pub fn clear_callbacks(&mut self) {
        self.on_configure = None;
        self.on_activate = None;
        self.on_deactivate = None;
        self.on_cleanup = None;
        self.on_shutdown = None;
        self.on_error = None;
        self.context = core::ptr::null_mut();
    }

    /// Force the state to `Finalized`. Used on fini.
    pub fn finalize(&mut self) {
        self.state = LifecycleState::Finalized;
    }

    /// Trigger a lifecycle transition.
    ///
    /// # Safety
    /// The registered callback (if any) is called via a raw `unsafe fn` pointer
    /// with the stored `*mut c_void` context. The caller must guarantee that
    /// any registered callback / context pair remains valid.
    pub unsafe fn trigger_transition(
        &mut self,
        transition: LifecycleTransition,
    ) -> Result<LifecycleState, LifecycleError> {
        if self.state.is_terminal() {
            return Err(LifecycleError::NodeFinalized);
        }

        if !can_transition(self.state, transition) {
            return Err(LifecycleError::InvalidTransition {
                from: self.state,
                transition,
            });
        }

        let cb = match transition {
            LifecycleTransition::Configure => self.on_configure,
            LifecycleTransition::Activate => self.on_activate,
            LifecycleTransition::Deactivate => self.on_deactivate,
            LifecycleTransition::Cleanup => self.on_cleanup,
            LifecycleTransition::ShutdownUnconfigured
            | LifecycleTransition::ShutdownInactive
            | LifecycleTransition::ShutdownActive => self.on_shutdown,
            LifecycleTransition::ErrorRecovery => self.on_error,
        };

        let result = match cb {
            Some(f) => {
                let raw = unsafe { f(self.context) };
                TransitionResult::from_u8(raw).unwrap_or(TransitionResult::Error)
            }
            None => TransitionResult::Success,
        };

        self.state = apply_transition(self.state, transition, result);

        if result == TransitionResult::Success {
            Ok(self.state)
        } else {
            Err(LifecycleError::CallbackFailed { transition, result })
        }
    }
}

impl Default for LifecyclePollingNodeCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// Which transition callback slot to register in [`LifecyclePollingNodeCtx::register`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCallbackSlot {
    /// `Unconfigured -> Inactive`
    Configure,
    /// `Inactive -> Active`
    Activate,
    /// `Active -> Inactive`
    Deactivate,
    /// `Inactive -> Unconfigured`
    Cleanup,
    /// any state -> `Finalized`
    Shutdown,
    /// `ErrorProcessing -> Unconfigured`
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════
    // LifecyclePollingNode tests (no_std, always available)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_polling_node_initial_state() {
        let node = LifecyclePollingNode::new();
        assert_eq!(node.state(), LifecycleState::Unconfigured);
    }

    #[test]
    fn test_polling_node_default() {
        let node = LifecyclePollingNode::default();
        assert_eq!(node.state(), LifecycleState::Unconfigured);
    }

    #[test]
    fn test_polling_node_happy_path() {
        let mut node = LifecyclePollingNode::new();

        assert_eq!(node.configure().unwrap(), LifecycleState::Inactive);
        assert_eq!(node.activate().unwrap(), LifecycleState::Active);
        assert_eq!(node.deactivate().unwrap(), LifecycleState::Inactive);
        assert_eq!(node.shutdown().unwrap(), LifecycleState::Finalized);
    }

    #[test]
    fn test_polling_node_cleanup_cycle() {
        let mut node = LifecyclePollingNode::new();

        node.configure().unwrap();
        assert_eq!(node.cleanup().unwrap(), LifecycleState::Unconfigured);

        // Can configure again
        assert_eq!(node.configure().unwrap(), LifecycleState::Inactive);
    }

    #[test]
    fn test_polling_node_invalid_transition() {
        let mut node = LifecyclePollingNode::new();

        let err = node.activate().unwrap_err();
        assert_eq!(
            err,
            LifecycleError::InvalidTransition {
                from: LifecycleState::Unconfigured,
                transition: LifecycleTransition::Activate,
            }
        );
    }

    #[test]
    fn test_polling_node_finalized_rejection() {
        let mut node = LifecyclePollingNode::new();
        node.shutdown().unwrap();

        assert_eq!(node.configure().unwrap_err(), LifecycleError::NodeFinalized);
        assert_eq!(node.shutdown().unwrap_err(), LifecycleError::NodeFinalized);
    }

    fn on_configure_success() -> TransitionResult {
        TransitionResult::Success
    }

    fn on_configure_failure() -> TransitionResult {
        TransitionResult::Failure
    }

    fn on_configure_error() -> TransitionResult {
        TransitionResult::Error
    }

    #[test]
    fn test_polling_node_callback_success() {
        let mut node = LifecyclePollingNode::new();
        node.register_on_configure(on_configure_success);

        assert_eq!(node.configure().unwrap(), LifecycleState::Inactive);
    }

    #[test]
    fn test_polling_node_callback_failure_rollback() {
        let mut node = LifecyclePollingNode::new();
        node.register_on_configure(on_configure_failure);

        let err = node.configure().unwrap_err();
        assert_eq!(
            err,
            LifecycleError::CallbackFailed {
                transition: LifecycleTransition::Configure,
                result: TransitionResult::Failure,
            }
        );
        // State rolled back to Unconfigured
        assert_eq!(node.state(), LifecycleState::Unconfigured);
    }

    #[test]
    fn test_polling_node_callback_error() {
        let mut node = LifecyclePollingNode::new();
        node.register_on_configure(on_configure_error);

        let err = node.configure().unwrap_err();
        assert_eq!(
            err,
            LifecycleError::CallbackFailed {
                transition: LifecycleTransition::Configure,
                result: TransitionResult::Error,
            }
        );
        // State moved to ErrorProcessing
        assert_eq!(node.state(), LifecycleState::ErrorProcessing);
    }

    #[test]
    fn test_polling_node_error_recovery() {
        let mut node = LifecyclePollingNode::new();
        node.register_on_configure(on_configure_error);

        let _ = node.configure();
        assert_eq!(node.state(), LifecycleState::ErrorProcessing);

        // Cannot shutdown from error processing
        assert!(node.shutdown().is_err());

        // Can recover
        node.register_on_error(on_configure_success);
        let result = node.trigger_transition(LifecycleTransition::ErrorRecovery);
        assert_eq!(result.unwrap(), LifecycleState::Unconfigured);
    }

    #[test]
    fn test_polling_node_bring_up() {
        let mut node = LifecyclePollingNode::new();
        assert_eq!(node.bring_up().unwrap(), LifecycleState::Active);
    }

    #[test]
    fn test_polling_node_bring_up_stops_on_configure_failure() {
        let mut node = LifecyclePollingNode::new();
        node.register_on_configure(on_configure_failure);

        let err = node.bring_up().unwrap_err();
        assert_eq!(
            err,
            LifecycleError::CallbackFailed {
                transition: LifecycleTransition::Configure,
                result: TransitionResult::Failure,
            }
        );
        // State is still Unconfigured, activate was never attempted
        assert_eq!(node.state(), LifecycleState::Unconfigured);
    }

    #[test]
    fn test_polling_node_shutdown_from_each_state() {
        // From Unconfigured
        let mut node = LifecyclePollingNode::new();
        assert_eq!(node.shutdown().unwrap(), LifecycleState::Finalized);

        // From Inactive
        let mut node = LifecyclePollingNode::new();
        node.configure().unwrap();
        assert_eq!(node.shutdown().unwrap(), LifecycleState::Finalized);

        // From Active
        let mut node = LifecyclePollingNode::new();
        node.bring_up().unwrap();
        assert_eq!(node.shutdown().unwrap(), LifecycleState::Finalized);
    }

    #[test]
    fn test_polling_node_no_callback_defaults_success() {
        // Without any callbacks registered, transitions should succeed
        let mut node = LifecyclePollingNode::new();
        assert_eq!(node.configure().unwrap(), LifecycleState::Inactive);
        assert_eq!(node.activate().unwrap(), LifecycleState::Active);
        assert_eq!(node.deactivate().unwrap(), LifecycleState::Inactive);
        assert_eq!(node.cleanup().unwrap(), LifecycleState::Unconfigured);
        assert_eq!(node.shutdown().unwrap(), LifecycleState::Finalized);
    }

    fn on_shutdown_success() -> TransitionResult {
        TransitionResult::Success
    }

    #[test]
    fn test_polling_node_shutdown_callback_invoked() {
        let mut node = LifecyclePollingNode::new();
        node.register_on_shutdown(on_shutdown_success);

        // Shutdown from Unconfigured should invoke the shutdown callback
        assert_eq!(node.shutdown().unwrap(), LifecycleState::Finalized);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // LifecyclePollingNodeCtx tests (C FFI shape)
    // ═══════════════════════════════════════════════════════════════════════

    unsafe extern "C" fn ctx_cb_success(_: *mut c_void) -> u8 {
        TransitionResult::Success as u8
    }
    unsafe extern "C" fn ctx_cb_failure(_: *mut c_void) -> u8 {
        TransitionResult::Failure as u8
    }
    unsafe extern "C" fn ctx_cb_error(_: *mut c_void) -> u8 {
        TransitionResult::Error as u8
    }

    #[test]
    fn test_ctx_node_happy_path() {
        unsafe {
            let mut node = LifecyclePollingNodeCtx::new();
            node.register(LifecycleCallbackSlot::Configure, Some(ctx_cb_success));
            node.register(LifecycleCallbackSlot::Activate, Some(ctx_cb_success));
            node.register(LifecycleCallbackSlot::Deactivate, Some(ctx_cb_success));
            node.register(LifecycleCallbackSlot::Shutdown, Some(ctx_cb_success));

            assert_eq!(
                node.trigger_transition(LifecycleTransition::Configure)
                    .unwrap(),
                LifecycleState::Inactive
            );
            assert_eq!(
                node.trigger_transition(LifecycleTransition::Activate)
                    .unwrap(),
                LifecycleState::Active
            );
            assert_eq!(
                node.trigger_transition(LifecycleTransition::Deactivate)
                    .unwrap(),
                LifecycleState::Inactive
            );
            assert_eq!(
                node.trigger_transition(LifecycleTransition::ShutdownInactive)
                    .unwrap(),
                LifecycleState::Finalized
            );
        }
    }

    #[test]
    fn test_ctx_node_invalid_transition() {
        unsafe {
            let mut node = LifecyclePollingNodeCtx::new();
            let err = node
                .trigger_transition(LifecycleTransition::Activate)
                .unwrap_err();
            assert_eq!(
                err,
                LifecycleError::InvalidTransition {
                    from: LifecycleState::Unconfigured,
                    transition: LifecycleTransition::Activate,
                }
            );
            assert_eq!(node.state(), LifecycleState::Unconfigured);
        }
    }

    #[test]
    fn test_ctx_node_callback_failure_rolls_back() {
        unsafe {
            let mut node = LifecyclePollingNodeCtx::new();
            node.register(LifecycleCallbackSlot::Configure, Some(ctx_cb_failure));
            assert!(
                node.trigger_transition(LifecycleTransition::Configure)
                    .is_err()
            );
            assert_eq!(node.state(), LifecycleState::Unconfigured);
        }
    }

    #[test]
    fn test_ctx_node_callback_error_enters_error_processing() {
        unsafe {
            let mut node = LifecyclePollingNodeCtx::new();
            node.register(LifecycleCallbackSlot::Configure, Some(ctx_cb_error));
            assert!(
                node.trigger_transition(LifecycleTransition::Configure)
                    .is_err()
            );
            assert_eq!(node.state(), LifecycleState::ErrorProcessing);
        }
    }

    #[test]
    fn test_ctx_node_finalized_rejects() {
        unsafe {
            let mut node = LifecyclePollingNodeCtx::new();
            node.finalize();
            let err = node
                .trigger_transition(LifecycleTransition::Configure)
                .unwrap_err();
            assert_eq!(err, LifecycleError::NodeFinalized);
        }
    }

    #[test]
    fn test_ctx_node_context_passed() {
        use core::sync::atomic::{AtomicU32, Ordering};
        static SEEN: AtomicU32 = AtomicU32::new(0);
        unsafe extern "C" fn cb_record(ctx: *mut c_void) -> u8 {
            SEEN.store(ctx as usize as u32, Ordering::Relaxed);
            TransitionResult::Success as u8
        }

        unsafe {
            let mut node = LifecyclePollingNodeCtx::new();
            node.set_context(0xBEEFu32 as usize as *mut c_void);
            node.register(LifecycleCallbackSlot::Configure, Some(cb_record));
            let _ = node.trigger_transition(LifecycleTransition::Configure);
            assert_eq!(SEEN.load(Ordering::Relaxed), 0xBEEF);
        }
    }

    #[test]
    fn test_ctx_node_clear_callbacks_resets() {
        unsafe {
            let mut node = LifecyclePollingNodeCtx::new();
            node.set_context(1usize as *mut c_void);
            node.register(LifecycleCallbackSlot::Configure, Some(ctx_cb_success));
            node.clear_callbacks();
            assert!(node.context().is_null());
            // With no callback, transition still succeeds (default = Success).
            assert_eq!(
                node.trigger_transition(LifecycleTransition::Configure)
                    .unwrap(),
                LifecycleState::Inactive
            );
        }
    }
}
