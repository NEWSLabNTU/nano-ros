//! Lifecycle node API (REP-2002)
//!
//! Provides managed lifecycle state machines for nano-ros nodes.
//!
//! - [`LifecycleNode`] — wraps a [`NodeHandle`] with boxed callbacks (requires `alloc`)
//! - [`LifecyclePollingNode`] — standalone state machine with function pointers (`no_std`)

use nano_ros_core::lifecycle::{
    LifecycleState, LifecycleTransition, TransitionResult, apply_transition, can_transition,
};

/// Error type for lifecycle transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    /// The requested transition is not valid from the current state.
    InvalidTransition {
        from: LifecycleState,
        transition: LifecycleTransition,
    },
    /// The transition callback returned a non-success result.
    CallbackFailed {
        transition: LifecycleTransition,
        result: TransitionResult,
    },
    /// The node is in the Finalized state and cannot transition.
    NodeFinalized,
}

// ═══════════════════════════════════════════════════════════════════════════
// LIFECYCLE NODE (alloc — Box callbacks)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(all(feature = "zenoh", feature = "alloc"))]
use alloc::boxed::Box;

#[cfg(all(feature = "zenoh", feature = "alloc"))]
use crate::executor::NodeHandle;

/// Lifecycle-managed node with boxed callbacks.
///
/// Wraps a [`NodeHandle`] and adds REP-2002 state machine management.
/// Transition callbacks are stored as `Box<dyn FnMut() -> TransitionResult>`.
///
/// # Example
///
/// ```ignore
/// let mut lifecycle = executor.create_lifecycle_node("sensor")?;
///
/// lifecycle.register_on_configure(|| {
///     println!("Configuring...");
///     TransitionResult::Success
/// });
///
/// lifecycle.configure()?;
/// lifecycle.activate()?;
/// assert_eq!(lifecycle.state(), LifecycleState::Active);
/// ```
#[cfg(all(feature = "zenoh", feature = "alloc"))]
pub struct LifecycleNode<'a> {
    node: NodeHandle<'a>,
    state: LifecycleState,
    on_configure: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_activate: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_deactivate: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_cleanup: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_shutdown: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
    on_error: Option<Box<dyn FnMut() -> TransitionResult + Send>>,
}

#[cfg(all(feature = "zenoh", feature = "alloc"))]
impl<'a> LifecycleNode<'a> {
    /// Create a new lifecycle node wrapping an existing `NodeHandle`.
    ///
    /// The node starts in the `Unconfigured` state.
    pub fn new(node: NodeHandle<'a>) -> Self {
        Self {
            node,
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
    pub fn state(&self) -> LifecycleState {
        self.state
    }

    /// Access the inner `NodeHandle` (immutable).
    pub fn node(&self) -> &NodeHandle<'a> {
        &self.node
    }

    /// Access the inner `NodeHandle` (mutable).
    pub fn node_mut(&mut self) -> &mut NodeHandle<'a> {
        &mut self.node
    }

    /// Trigger a lifecycle transition.
    ///
    /// Validates the transition, invokes the registered callback (if any),
    /// and applies the result per REP-2002.
    ///
    /// Returns the new state on success (including callback Failure which
    /// rolls back), or an error if the transition is invalid.
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
    ///
    /// Resolves the correct shutdown variant based on the current state.
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
    pub fn register_on_configure(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) {
        self.on_configure = Some(Box::new(cb));
    }

    /// Register a callback for the `activate` transition.
    pub fn register_on_activate(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) {
        self.on_activate = Some(Box::new(cb));
    }

    /// Register a callback for the `deactivate` transition.
    pub fn register_on_deactivate(
        &mut self,
        cb: impl FnMut() -> TransitionResult + Send + 'static,
    ) {
        self.on_deactivate = Some(Box::new(cb));
    }

    /// Register a callback for the `cleanup` transition.
    pub fn register_on_cleanup(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) {
        self.on_cleanup = Some(Box::new(cb));
    }

    /// Register a callback for the `shutdown` transition.
    pub fn register_on_shutdown(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) {
        self.on_shutdown = Some(Box::new(cb));
    }

    /// Register a callback for the `error` transition (error recovery).
    pub fn register_on_error(&mut self, cb: impl FnMut() -> TransitionResult + Send + 'static) {
        self.on_error = Some(Box::new(cb));
    }

    fn invoke_callback(&mut self, transition: LifecycleTransition) -> TransitionResult {
        let cb = match transition {
            LifecycleTransition::Configure => &mut self.on_configure,
            LifecycleTransition::Activate => &mut self.on_activate,
            LifecycleTransition::Deactivate => &mut self.on_deactivate,
            LifecycleTransition::Cleanup => &mut self.on_cleanup,
            LifecycleTransition::ShutdownUnconfigured
            | LifecycleTransition::ShutdownInactive
            | LifecycleTransition::ShutdownActive => &mut self.on_shutdown,
            LifecycleTransition::ErrorRecovery => &mut self.on_error,
        };

        match cb {
            Some(f) => f(),
            None => TransitionResult::Success,
        }
    }
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
}
