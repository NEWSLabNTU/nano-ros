//! Lifecycle state machine types (REP-2002)
//!
//! Provides the core types for ROS 2 lifecycle node management:
//! - [`LifecycleState`] — the five lifecycle states
//! - [`LifecycleTransition`] — transitions between states
//! - [`TransitionResult`] — callback return values
//!
//! These types are shared between the Rust and C APIs. All enums use
//! `#[repr(u8)]` for C interop.

/// Lifecycle state (REP-2002)
///
/// A lifecycle node is always in exactly one of these states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LifecycleState {
    /// Initial state after construction. Node is not yet configured.
    Unconfigured = 1,
    /// Node is configured but not processing data.
    Inactive = 2,
    /// Node is fully operational and processing data.
    Active = 3,
    /// Terminal state. Node cannot be reused.
    Finalized = 4,
    /// Error occurred during a transition. Must recover or shut down.
    ErrorProcessing = 5,
}

impl LifecycleState {
    /// Returns true if this is a terminal state (Finalized).
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Finalized)
    }

    /// Returns true if this is a primary state (not a transition state).
    ///
    /// Primary states are: Unconfigured, Inactive, Active, Finalized.
    /// ErrorProcessing is the only non-primary state.
    pub const fn is_primary(&self) -> bool {
        !matches!(self, Self::ErrorProcessing)
    }

    /// Try to convert from a u8 value.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Unconfigured),
            2 => Some(Self::Inactive),
            3 => Some(Self::Active),
            4 => Some(Self::Finalized),
            5 => Some(Self::ErrorProcessing),
            _ => None,
        }
    }

    /// Every reachable state, in `lifecycle_msgs` id order.
    ///
    /// phase-417 W4.f. `nros_node::lifecycle_services` held this list
    /// privately, so the wire could enumerate the states and no in-process
    /// caller in any of the three languages could. It is a CONSTANT, not
    /// state — the point of moving it here is that there is one of it.
    pub const ALL: [Self; 5] = [
        Self::Unconfigured,
        Self::Inactive,
        Self::Active,
        Self::Finalized,
        Self::ErrorProcessing,
    ];

    /// The `lifecycle_msgs/msg/State.label` for this state, NUL-terminated.
    ///
    /// The C string is the SOURCE spelling and [`Self::label`] is derived from
    /// it, not the other way round: the C API hands this pointer straight to a
    /// caller (`nros_lifecycle_state_label`), and a second set of literals for
    /// the `&str` form is exactly the drift this move exists to remove.
    pub const fn label_cstr(&self) -> &'static core::ffi::CStr {
        match self {
            Self::Unconfigured => c"unconfigured",
            Self::Inactive => c"inactive",
            Self::Active => c"active",
            Self::Finalized => c"finalized",
            Self::ErrorProcessing => c"errorprocessing",
        }
    }

    /// The `lifecycle_msgs/msg/State.label` for this state.
    ///
    /// Infallible in practice — every label above is ASCII — but expressed
    /// without an `unwrap`, because a panic on a `const` table is a panic
    /// nobody can act on. The empty string is unreachable, and
    /// `every_label_is_valid_utf8` is the test that keeps it so.
    pub fn label(&self) -> &'static str {
        self.label_cstr().to_str().unwrap_or_default()
    }
}

/// Lifecycle transition (REP-2002)
///
/// Each transition has a specific source state. Shutdown has three variants
/// because it can originate from Unconfigured, Inactive, or Active.
///
/// # The discriminants ARE `lifecycle_msgs/msg/Transition` (issue 1099)
///
/// These numbers are not ours to choose. They cross the C ABI as
/// `NROS_LIFECYCLE_TRANSITION_*`, they are what
/// `nros::LifecycleNode::trigger_transition(uint8_t)` accepts, and they are
/// what a `ChangeState` request carries on the wire — so a caller reaching any
/// one of those surfaces with a literal must get the transition ROS 2 would
/// give them.
///
/// They did not, until issue 1099: `Activate`/`Deactivate`/`Cleanup` were
/// `2`/`3`/`4` where upstream is `3`/`4`/`2`, so a ported
/// `trigger_transition(2)` on an Inactive node ACTIVATED it and returned OK
/// where ROS 2 cleans it up. Four of eight ids disagreed, and only the wire
/// path translated — the C and C++ paths passed the raw byte through.
///
/// The one id with no upstream counterpart is [`Self::ErrorRecovery`]: upstream
/// `8` is `TRANSITION_DESTROY`, which we do not implement (nothing is
/// deallocated in a fixed arena), and upstream models error recovery as the
/// IMPLICIT `TRANSITION_ON_ERROR_SUCCESS = 60`. So `ErrorRecovery` is `60` —
/// the id `transition_wire()` in `nros-node` already put on the wire for it —
/// and `8` is now rejected rather than silently meaning error recovery.
///
/// Do not renumber these. `nros_c::lifecycle` re-asserts every one against its
/// `NROS_LIFECYCLE_TRANSITION_*` literal (issue 0792), and `nros-node`'s
/// `transition_wire` / `from_msg_transition_id` are now identity functions that
/// a test pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LifecycleTransition {
    /// Unconfigured -> (configuring) -> Inactive
    /// (`lifecycle_msgs` `TRANSITION_CONFIGURE`)
    Configure = 1,
    /// Inactive -> (cleaning up) -> Unconfigured
    /// (`lifecycle_msgs` `TRANSITION_CLEANUP`)
    Cleanup = 2,
    /// Inactive -> (activating) -> Active
    /// (`lifecycle_msgs` `TRANSITION_ACTIVATE`)
    Activate = 3,
    /// Active -> (deactivating) -> Inactive
    /// (`lifecycle_msgs` `TRANSITION_DEACTIVATE`)
    Deactivate = 4,
    /// Unconfigured -> (shutting down) -> Finalized
    /// (`lifecycle_msgs` `TRANSITION_UNCONFIGURED_SHUTDOWN`)
    ShutdownUnconfigured = 5,
    /// Inactive -> (shutting down) -> Finalized
    /// (`lifecycle_msgs` `TRANSITION_INACTIVE_SHUTDOWN`)
    ShutdownInactive = 6,
    /// Active -> (shutting down) -> Finalized
    /// (`lifecycle_msgs` `TRANSITION_ACTIVE_SHUTDOWN`)
    ShutdownActive = 7,
    /// ErrorProcessing -> (error recovery) -> Unconfigured
    /// (`lifecycle_msgs` `TRANSITION_ON_ERROR_SUCCESS`; see the type doc for
    /// why this is 60 and not 8 — 8 is upstream's `TRANSITION_DESTROY`.)
    ErrorRecovery = 60,
}

impl LifecycleTransition {
    /// Resolve a shorthand transition name from the current state.
    ///
    /// "shutdown" maps to the correct variant based on the current state.
    /// Returns `None` if the shorthand is not valid from the given state.
    pub fn from_shorthand(state: LifecycleState, name: &str) -> Option<Self> {
        match name {
            "configure" => Some(Self::Configure),
            "activate" => Some(Self::Activate),
            "deactivate" => Some(Self::Deactivate),
            "cleanup" => Some(Self::Cleanup),
            "shutdown" => match state {
                LifecycleState::Unconfigured => Some(Self::ShutdownUnconfigured),
                LifecycleState::Inactive => Some(Self::ShutdownInactive),
                LifecycleState::Active => Some(Self::ShutdownActive),
                _ => None,
            },
            "error_recovery" => Some(Self::ErrorRecovery),
            _ => None,
        }
    }

    /// Try to convert from a `lifecycle_msgs/msg/Transition` id.
    ///
    /// `0` (`TRANSITION_CREATE`) and `8` (`TRANSITION_DESTROY`) are upstream
    /// ids we deliberately do not implement — construction and deallocation are
    /// not transitions in a fixed arena — so both are `None` rather than
    /// aliases for something else (issue 1099).
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Configure),
            2 => Some(Self::Cleanup),
            3 => Some(Self::Activate),
            4 => Some(Self::Deactivate),
            5 => Some(Self::ShutdownUnconfigured),
            6 => Some(Self::ShutdownInactive),
            7 => Some(Self::ShutdownActive),
            60 => Some(Self::ErrorRecovery),
            _ => None,
        }
    }

    /// Get the source state required for this transition.
    pub const fn source_state(&self) -> LifecycleState {
        match self {
            Self::Configure => LifecycleState::Unconfigured,
            Self::Activate => LifecycleState::Inactive,
            Self::Deactivate => LifecycleState::Active,
            Self::Cleanup => LifecycleState::Inactive,
            Self::ShutdownUnconfigured => LifecycleState::Unconfigured,
            Self::ShutdownInactive => LifecycleState::Inactive,
            Self::ShutdownActive => LifecycleState::Active,
            Self::ErrorRecovery => LifecycleState::ErrorProcessing,
        }
    }

    /// Every transition in the REP-2002 graph, in `lifecycle_msgs` id order.
    ///
    /// phase-417 W4.f. The three shutdown variants are listed separately
    /// because their [`source_state`](Self::source_state) differs, which is
    /// also rclcpp's graph shape. `nros_node::lifecycle_services` served this
    /// table over `~/get_transition_graph` while holding it privately, so a
    /// node's own code could not read its transition table in ANY of our three
    /// languages; this is the one table all of them now read.
    pub const ALL: [Self; 8] = [
        Self::Configure,
        Self::Cleanup,
        Self::Activate,
        Self::Deactivate,
        Self::ShutdownUnconfigured,
        Self::ShutdownInactive,
        Self::ShutdownActive,
        Self::ErrorRecovery,
    ];

    /// The `lifecycle_msgs/msg/Transition.label` for this transition,
    /// NUL-terminated. See [`LifecycleState::label_cstr`] for why the C
    /// spelling is the source.
    ///
    /// The three shutdown variants share the label `"shutdown"` — that is
    /// upstream's wire spelling, and `from_shorthand` is what resolves it back
    /// against the current state.
    pub const fn label_cstr(&self) -> &'static core::ffi::CStr {
        match self {
            Self::Configure => c"configure",
            Self::Cleanup => c"cleanup",
            Self::Activate => c"activate",
            Self::Deactivate => c"deactivate",
            Self::ShutdownUnconfigured => c"shutdown",
            Self::ShutdownInactive => c"shutdown",
            Self::ShutdownActive => c"shutdown",
            Self::ErrorRecovery => c"error_recovery",
        }
    }

    /// The `lifecycle_msgs/msg/Transition.label` for this transition.
    pub fn label(&self) -> &'static str {
        self.label_cstr().to_str().unwrap_or_default()
    }

    /// The state this transition ADVERTISES as its destination — rclcpp's
    /// `Transition::goal_state()`.
    ///
    /// It is the state a SUCCEEDING callback reaches, which is what the
    /// transition graph advertises. A failing callback routes to
    /// `ErrorProcessing` or rolls back at runtime; that is
    /// [`apply_transition`]'s business and orthogonal to the advertised graph.
    pub const fn goal_state(&self) -> LifecycleState {
        apply_transition(self.source_state(), *self, TransitionResult::Success)
    }
}

/// Result of a lifecycle transition callback.
///
/// Matches the rclc convention where callbacks return a status
/// indicating whether the transition should proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TransitionResult {
    /// Transition succeeded; move to the target state.
    Success = 0,
    /// Transition failed; roll back to the previous primary state.
    Failure = 1,
    /// An error occurred; move to ErrorProcessing state.
    Error = 2,
}

impl TransitionResult {
    /// Try to convert from a u8 value.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Success),
            1 => Some(Self::Failure),
            2 => Some(Self::Error),
            _ => None,
        }
    }
}

/// Check whether a transition is valid from the given state.
pub const fn can_transition(state: LifecycleState, transition: LifecycleTransition) -> bool {
    matches!(
        (state, transition),
        (LifecycleState::Unconfigured, LifecycleTransition::Configure)
            | (
                LifecycleState::Unconfigured,
                LifecycleTransition::ShutdownUnconfigured
            )
            | (LifecycleState::Inactive, LifecycleTransition::Activate)
            | (LifecycleState::Inactive, LifecycleTransition::Cleanup)
            | (
                LifecycleState::Inactive,
                LifecycleTransition::ShutdownInactive
            )
            | (LifecycleState::Active, LifecycleTransition::Deactivate)
            | (LifecycleState::Active, LifecycleTransition::ShutdownActive)
            | (
                LifecycleState::ErrorProcessing,
                LifecycleTransition::ErrorRecovery
            )
    )
}

/// Apply a transition given the callback result.
///
/// Implements the REP-2002 transition table:
/// - **Success**: move to the target state
/// - **Failure**: roll back to the previous primary state
/// - **Error**: move to ErrorProcessing
pub const fn apply_transition(
    state: LifecycleState,
    transition: LifecycleTransition,
    result: TransitionResult,
) -> LifecycleState {
    match result {
        TransitionResult::Error => LifecycleState::ErrorProcessing,
        TransitionResult::Success => match transition {
            LifecycleTransition::Configure => LifecycleState::Inactive,
            LifecycleTransition::Activate => LifecycleState::Active,
            LifecycleTransition::Deactivate => LifecycleState::Inactive,
            LifecycleTransition::Cleanup => LifecycleState::Unconfigured,
            LifecycleTransition::ShutdownUnconfigured
            | LifecycleTransition::ShutdownInactive
            | LifecycleTransition::ShutdownActive => LifecycleState::Finalized,
            LifecycleTransition::ErrorRecovery => LifecycleState::Unconfigured,
        },
        TransitionResult::Failure => {
            // Roll back to the source state (previous primary state)
            // For error recovery, failure means we stay in ErrorProcessing
            // since there's nowhere to roll back to
            match transition {
                LifecycleTransition::ErrorRecovery => LifecycleState::ErrorProcessing,
                _ => state,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_state_properties() {
        assert!(!LifecycleState::Unconfigured.is_terminal());
        assert!(!LifecycleState::Inactive.is_terminal());
        assert!(!LifecycleState::Active.is_terminal());
        assert!(LifecycleState::Finalized.is_terminal());
        assert!(!LifecycleState::ErrorProcessing.is_terminal());

        assert!(LifecycleState::Unconfigured.is_primary());
        assert!(LifecycleState::Inactive.is_primary());
        assert!(LifecycleState::Active.is_primary());
        assert!(LifecycleState::Finalized.is_primary());
        assert!(!LifecycleState::ErrorProcessing.is_primary());
    }

    #[test]
    fn test_state_from_u8() {
        assert_eq!(
            LifecycleState::from_u8(1),
            Some(LifecycleState::Unconfigured)
        );
        assert_eq!(LifecycleState::from_u8(2), Some(LifecycleState::Inactive));
        assert_eq!(LifecycleState::from_u8(3), Some(LifecycleState::Active));
        assert_eq!(LifecycleState::from_u8(4), Some(LifecycleState::Finalized));
        assert_eq!(
            LifecycleState::from_u8(5),
            Some(LifecycleState::ErrorProcessing)
        );
        assert_eq!(LifecycleState::from_u8(0), None);
        assert_eq!(LifecycleState::from_u8(6), None);
    }

    /// Issue 1099 — the discriminants ARE `lifecycle_msgs/msg/Transition`.
    ///
    /// The table below is transcribed from
    /// `/opt/ros/humble/share/lifecycle_msgs/msg/Transition.msg`; it is the
    /// contract every id-taking surface inherits (`NROS_LIFECYCLE_TRANSITION_*`,
    /// `nros::LifecycleNode::trigger_transition`, `ChangeState.transition.id`).
    ///
    /// Before the fix this failed on FOUR of eight rows: `2` was `Activate`,
    /// `3` was `Deactivate`, `4` was `Cleanup`, `8` was `ErrorRecovery`. The
    /// `(2, Cleanup)` row is the dangerous one — `trigger_transition(2)` on an
    /// Inactive node ACTIVATED it and returned OK.
    #[test]
    fn transition_discriminants_are_lifecycle_msgs_ids() {
        // (upstream id, upstream constant name, our variant)
        let table = [
            (1u8, "TRANSITION_CONFIGURE", LifecycleTransition::Configure),
            (2, "TRANSITION_CLEANUP", LifecycleTransition::Cleanup),
            (3, "TRANSITION_ACTIVATE", LifecycleTransition::Activate),
            (4, "TRANSITION_DEACTIVATE", LifecycleTransition::Deactivate),
            (
                5,
                "TRANSITION_UNCONFIGURED_SHUTDOWN",
                LifecycleTransition::ShutdownUnconfigured,
            ),
            (
                6,
                "TRANSITION_INACTIVE_SHUTDOWN",
                LifecycleTransition::ShutdownInactive,
            ),
            (
                7,
                "TRANSITION_ACTIVE_SHUTDOWN",
                LifecycleTransition::ShutdownActive,
            ),
            (
                60,
                "TRANSITION_ON_ERROR_SUCCESS",
                LifecycleTransition::ErrorRecovery,
            ),
        ];
        for (id, name, variant) in table {
            assert_eq!(
                variant as u8, id,
                "{variant:?} must be lifecycle_msgs::{name} = {id}"
            );
            assert_eq!(
                LifecycleTransition::from_u8(id),
                Some(variant),
                "lifecycle_msgs::{name} = {id} must decode to {variant:?}"
            );
        }
    }

    /// The two upstream ids we deliberately do not implement must be REJECTED,
    /// not silently aliased onto a transition we do have (issue 1099).
    #[test]
    fn unimplemented_upstream_transition_ids_are_rejected() {
        // TRANSITION_CREATE = 0 and TRANSITION_DESTROY = 8: construction and
        // deallocation are not transitions in a fixed arena. `8` used to mean
        // `ErrorRecovery` here.
        assert_eq!(LifecycleTransition::from_u8(0), None);
        assert_eq!(LifecycleTransition::from_u8(8), None);
        // Not an id at all.
        assert_eq!(LifecycleTransition::from_u8(9), None);
    }

    /// Every transition round-trips through its id, and no two share one.
    #[test]
    fn transition_ids_are_injective_and_round_trip() {
        let all = [
            LifecycleTransition::Configure,
            LifecycleTransition::Cleanup,
            LifecycleTransition::Activate,
            LifecycleTransition::Deactivate,
            LifecycleTransition::ShutdownUnconfigured,
            LifecycleTransition::ShutdownInactive,
            LifecycleTransition::ShutdownActive,
            LifecycleTransition::ErrorRecovery,
        ];
        for (i, a) in all.iter().enumerate() {
            assert_eq!(LifecycleTransition::from_u8(*a as u8), Some(*a));
            for b in &all[i + 1..] {
                assert_ne!(*a as u8, *b as u8, "{a:?} and {b:?} share an id");
            }
        }
    }

    #[test]
    fn test_transition_result_from_u8() {
        assert_eq!(
            TransitionResult::from_u8(0),
            Some(TransitionResult::Success)
        );
        assert_eq!(
            TransitionResult::from_u8(1),
            Some(TransitionResult::Failure)
        );
        assert_eq!(TransitionResult::from_u8(2), Some(TransitionResult::Error));
        assert_eq!(TransitionResult::from_u8(3), None);
    }

    #[test]
    fn test_valid_transitions() {
        // From Unconfigured
        assert!(can_transition(
            LifecycleState::Unconfigured,
            LifecycleTransition::Configure
        ));
        assert!(can_transition(
            LifecycleState::Unconfigured,
            LifecycleTransition::ShutdownUnconfigured
        ));
        assert!(!can_transition(
            LifecycleState::Unconfigured,
            LifecycleTransition::Activate
        ));
        assert!(!can_transition(
            LifecycleState::Unconfigured,
            LifecycleTransition::Deactivate
        ));

        // From Inactive
        assert!(can_transition(
            LifecycleState::Inactive,
            LifecycleTransition::Activate
        ));
        assert!(can_transition(
            LifecycleState::Inactive,
            LifecycleTransition::Cleanup
        ));
        assert!(can_transition(
            LifecycleState::Inactive,
            LifecycleTransition::ShutdownInactive
        ));
        assert!(!can_transition(
            LifecycleState::Inactive,
            LifecycleTransition::Configure
        ));
        assert!(!can_transition(
            LifecycleState::Inactive,
            LifecycleTransition::Deactivate
        ));

        // From Active
        assert!(can_transition(
            LifecycleState::Active,
            LifecycleTransition::Deactivate
        ));
        assert!(can_transition(
            LifecycleState::Active,
            LifecycleTransition::ShutdownActive
        ));
        assert!(!can_transition(
            LifecycleState::Active,
            LifecycleTransition::Activate
        ));
        assert!(!can_transition(
            LifecycleState::Active,
            LifecycleTransition::Configure
        ));

        // From Finalized (terminal — no transitions)
        assert!(!can_transition(
            LifecycleState::Finalized,
            LifecycleTransition::Configure
        ));
        assert!(!can_transition(
            LifecycleState::Finalized,
            LifecycleTransition::ShutdownUnconfigured
        ));

        // From ErrorProcessing
        assert!(can_transition(
            LifecycleState::ErrorProcessing,
            LifecycleTransition::ErrorRecovery
        ));
        assert!(!can_transition(
            LifecycleState::ErrorProcessing,
            LifecycleTransition::Configure
        ));
    }

    #[test]
    fn test_apply_transition_success() {
        assert_eq!(
            apply_transition(
                LifecycleState::Unconfigured,
                LifecycleTransition::Configure,
                TransitionResult::Success
            ),
            LifecycleState::Inactive
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Inactive,
                LifecycleTransition::Activate,
                TransitionResult::Success
            ),
            LifecycleState::Active
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Active,
                LifecycleTransition::Deactivate,
                TransitionResult::Success
            ),
            LifecycleState::Inactive
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Inactive,
                LifecycleTransition::Cleanup,
                TransitionResult::Success
            ),
            LifecycleState::Unconfigured
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Unconfigured,
                LifecycleTransition::ShutdownUnconfigured,
                TransitionResult::Success
            ),
            LifecycleState::Finalized
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Inactive,
                LifecycleTransition::ShutdownInactive,
                TransitionResult::Success
            ),
            LifecycleState::Finalized
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Active,
                LifecycleTransition::ShutdownActive,
                TransitionResult::Success
            ),
            LifecycleState::Finalized
        );
        assert_eq!(
            apply_transition(
                LifecycleState::ErrorProcessing,
                LifecycleTransition::ErrorRecovery,
                TransitionResult::Success
            ),
            LifecycleState::Unconfigured
        );
    }

    #[test]
    fn test_apply_transition_failure_rolls_back() {
        // Failure on configure -> stay Unconfigured
        assert_eq!(
            apply_transition(
                LifecycleState::Unconfigured,
                LifecycleTransition::Configure,
                TransitionResult::Failure
            ),
            LifecycleState::Unconfigured
        );
        // Failure on activate -> stay Inactive
        assert_eq!(
            apply_transition(
                LifecycleState::Inactive,
                LifecycleTransition::Activate,
                TransitionResult::Failure
            ),
            LifecycleState::Inactive
        );
        // Failure on deactivate -> stay Active
        assert_eq!(
            apply_transition(
                LifecycleState::Active,
                LifecycleTransition::Deactivate,
                TransitionResult::Failure
            ),
            LifecycleState::Active
        );
        // Failure on error recovery -> stay ErrorProcessing
        assert_eq!(
            apply_transition(
                LifecycleState::ErrorProcessing,
                LifecycleTransition::ErrorRecovery,
                TransitionResult::Failure
            ),
            LifecycleState::ErrorProcessing
        );
    }

    #[test]
    fn test_apply_transition_error_goes_to_error_processing() {
        assert_eq!(
            apply_transition(
                LifecycleState::Unconfigured,
                LifecycleTransition::Configure,
                TransitionResult::Error
            ),
            LifecycleState::ErrorProcessing
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Inactive,
                LifecycleTransition::Activate,
                TransitionResult::Error
            ),
            LifecycleState::ErrorProcessing
        );
        assert_eq!(
            apply_transition(
                LifecycleState::Active,
                LifecycleTransition::ShutdownActive,
                TransitionResult::Error
            ),
            LifecycleState::ErrorProcessing
        );
    }

    #[test]
    fn test_shutdown_shorthand_disambiguation() {
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Unconfigured, "shutdown"),
            Some(LifecycleTransition::ShutdownUnconfigured)
        );
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Inactive, "shutdown"),
            Some(LifecycleTransition::ShutdownInactive)
        );
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Active, "shutdown"),
            Some(LifecycleTransition::ShutdownActive)
        );
        // Cannot shutdown from Finalized or ErrorProcessing
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Finalized, "shutdown"),
            None
        );
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::ErrorProcessing, "shutdown"),
            None
        );
    }

    #[test]
    fn test_shorthand_other_transitions() {
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Unconfigured, "configure"),
            Some(LifecycleTransition::Configure)
        );
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Active, "deactivate"),
            Some(LifecycleTransition::Deactivate)
        );
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Inactive, "cleanup"),
            Some(LifecycleTransition::Cleanup)
        );
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::ErrorProcessing, "error_recovery"),
            Some(LifecycleTransition::ErrorRecovery)
        );
        assert_eq!(
            LifecycleTransition::from_shorthand(LifecycleState::Active, "unknown"),
            None
        );
    }

    #[test]
    fn test_transition_source_state() {
        assert_eq!(
            LifecycleTransition::Configure.source_state(),
            LifecycleState::Unconfigured
        );
        assert_eq!(
            LifecycleTransition::Activate.source_state(),
            LifecycleState::Inactive
        );
        assert_eq!(
            LifecycleTransition::Deactivate.source_state(),
            LifecycleState::Active
        );
        assert_eq!(
            LifecycleTransition::Cleanup.source_state(),
            LifecycleState::Inactive
        );
        assert_eq!(
            LifecycleTransition::ShutdownUnconfigured.source_state(),
            LifecycleState::Unconfigured
        );
        assert_eq!(
            LifecycleTransition::ShutdownInactive.source_state(),
            LifecycleState::Inactive
        );
        assert_eq!(
            LifecycleTransition::ShutdownActive.source_state(),
            LifecycleState::Active
        );
        assert_eq!(
            LifecycleTransition::ErrorRecovery.source_state(),
            LifecycleState::ErrorProcessing
        );
    }

    #[test]
    fn test_full_lifecycle_happy_path() {
        let mut state = LifecycleState::Unconfigured;

        // Configure
        assert!(can_transition(state, LifecycleTransition::Configure));
        state = apply_transition(
            state,
            LifecycleTransition::Configure,
            TransitionResult::Success,
        );
        assert_eq!(state, LifecycleState::Inactive);

        // Activate
        assert!(can_transition(state, LifecycleTransition::Activate));
        state = apply_transition(
            state,
            LifecycleTransition::Activate,
            TransitionResult::Success,
        );
        assert_eq!(state, LifecycleState::Active);

        // Deactivate
        assert!(can_transition(state, LifecycleTransition::Deactivate));
        state = apply_transition(
            state,
            LifecycleTransition::Deactivate,
            TransitionResult::Success,
        );
        assert_eq!(state, LifecycleState::Inactive);

        // Shutdown
        assert!(can_transition(state, LifecycleTransition::ShutdownInactive));
        state = apply_transition(
            state,
            LifecycleTransition::ShutdownInactive,
            TransitionResult::Success,
        );
        assert_eq!(state, LifecycleState::Finalized);
        assert!(state.is_terminal());
    }

    #[test]
    fn test_error_recovery_path() {
        let mut state = LifecycleState::Unconfigured;

        // Configure with error
        state = apply_transition(
            state,
            LifecycleTransition::Configure,
            TransitionResult::Error,
        );
        assert_eq!(state, LifecycleState::ErrorProcessing);
        assert!(!state.is_primary());

        // Error recovery
        assert!(can_transition(state, LifecycleTransition::ErrorRecovery));
        state = apply_transition(
            state,
            LifecycleTransition::ErrorRecovery,
            TransitionResult::Success,
        );
        assert_eq!(state, LifecycleState::Unconfigured);
    }

    // ── phase-417 W4.f — the transition table as a readable fact ───────────

    /// Every label is ASCII, so the `&str` accessor never falls through to the
    /// empty string. Pinned because `label()` reports the unreachable arm
    /// SILENTLY rather than panicking: without this the fallback could become
    /// live and nothing would say so.
    #[test]
    fn every_label_is_valid_utf8() {
        for t in LifecycleTransition::ALL {
            assert!(!t.label().is_empty(), "transition {t:?} has no label");
            assert_eq!(t.label().as_bytes(), t.label_cstr().to_bytes());
        }
        for s in LifecycleState::ALL {
            assert!(!s.label().is_empty(), "state {s:?} has no label");
            assert_eq!(s.label().as_bytes(), s.label_cstr().to_bytes());
        }
    }

    /// `ALL` is the graph, so it must hold every variant exactly once and in
    /// id order. A table that silently lost a transition would serve a
    /// `~/get_transition_graph` short by one and read as correct.
    #[test]
    fn all_is_complete_and_ordered() {
        let ids: heapless::Vec<u8, 8> = LifecycleTransition::ALL
            .iter()
            .map(|t| *t as u8)
            .collect::<heapless::Vec<u8, 8>>();
        assert_eq!(ids.as_slice(), &[1, 2, 3, 4, 5, 6, 7, 60]);
        for id in ids.iter() {
            assert!(LifecycleTransition::from_u8(*id).is_some());
        }
        let state_ids: heapless::Vec<u8, 5> = LifecycleState::ALL
            .iter()
            .map(|s| *s as u8)
            .collect::<heapless::Vec<u8, 5>>();
        assert_eq!(state_ids.as_slice(), &[1, 2, 3, 4, 5]);
    }

    /// `goal_state` is the SUCCESS destination, and the three shutdowns all
    /// reach `Finalized` from three different sources — the pair that makes
    /// the graph worth publishing.
    #[test]
    fn goal_state_is_the_success_destination() {
        use LifecycleState as S;
        use LifecycleTransition as T;
        assert_eq!(T::Configure.goal_state(), S::Inactive);
        assert_eq!(T::Cleanup.goal_state(), S::Unconfigured);
        assert_eq!(T::Activate.goal_state(), S::Active);
        assert_eq!(T::Deactivate.goal_state(), S::Inactive);
        assert_eq!(T::ErrorRecovery.goal_state(), S::Unconfigured);
        for t in [
            T::ShutdownUnconfigured,
            T::ShutdownInactive,
            T::ShutdownActive,
        ] {
            assert_eq!(t.goal_state(), S::Finalized);
        }
        assert_eq!(T::ShutdownUnconfigured.source_state(), S::Unconfigured);
        assert_eq!(T::ShutdownInactive.source_state(), S::Inactive);
        assert_eq!(T::ShutdownActive.source_state(), S::Active);
    }
}
