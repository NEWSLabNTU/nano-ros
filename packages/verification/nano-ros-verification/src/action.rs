/// GoalStatus state machine proofs (Phase 31.4)
///
/// Proves correctness of the ROS 2 action GoalStatus lifecycle: terminal/active
/// disjointness, status code round-trip, and transition DAG structure.
///
/// ## Trust levels
///
/// **Formally linked** (via `assume_specification` + `external_type_specification`):
/// - `GoalStatus` — transparent enum registered via `external_type_specification`
///   (without `external_body`). Verus can match on all 7 variants directly.
/// - `GoalStatus::is_terminal()` — linked to `is_terminal_spec` via `assume_specification`.
/// - `GoalStatus::is_active()` — linked to `is_active_spec` via `assume_specification`.
/// - `GoalStatus::from_i8()` — linked to `from_i8_spec` via `assume_specification`.
///
/// **Pure math** (no link to production code):
/// - `transition_validity` — transition DAG property proved via ranking function.
use vstd::prelude::*;
use nano_ros_core::GoalStatus;

verus! {

// ======================================================================
// GoalStatus Type Specification
// ======================================================================

/// Register `GoalStatus` with Verus as a transparent type.
///
/// Without `external_body`, Verus sees the enum's variant structure and allows
/// pattern matching on Unknown, Accepted, Executing, Canceling, Succeeded,
/// Canceled, Aborted.
///
/// Source (action.rs:62-84):
/// ```ignore
/// #[repr(i8)]
/// pub enum GoalStatus {
///     Unknown = 0, Accepted = 1, Executing = 2, Canceling = 3,
///     Succeeded = 4, Canceled = 5, Aborted = 6,
/// }
/// ```
#[verifier::external_type_specification]
pub struct ExGoalStatus(GoalStatus);

// ======================================================================
// Spec Functions
// ======================================================================

/// Spec: `GoalStatus::is_terminal()` — true for completed states.
///
/// Mirrors (action.rs:87-93):
/// ```ignore
/// pub fn is_terminal(&self) -> bool {
///     matches!(self, GoalStatus::Succeeded | GoalStatus::Canceled | GoalStatus::Aborted)
/// }
/// ```
pub open spec fn is_terminal_spec(s: GoalStatus) -> bool {
    match s {
        GoalStatus::Succeeded | GoalStatus::Canceled | GoalStatus::Aborted => true,
        _ => false,
    }
}

/// Spec: `GoalStatus::is_active()` — true for in-progress states.
///
/// Mirrors (action.rs:95-101):
/// ```ignore
/// pub fn is_active(&self) -> bool {
///     matches!(self, GoalStatus::Accepted | GoalStatus::Executing | GoalStatus::Canceling)
/// }
/// ```
pub open spec fn is_active_spec(s: GoalStatus) -> bool {
    match s {
        GoalStatus::Accepted | GoalStatus::Executing | GoalStatus::Canceling => true,
        _ => false,
    }
}

/// Spec: `GoalStatus as i8` — the repr(i8) discriminant.
///
/// Mirrors the `#[repr(i8)]` discriminant values on the enum variants.
pub open spec fn to_i8_spec(s: GoalStatus) -> i8 {
    match s {
        GoalStatus::Unknown => 0i8,
        GoalStatus::Accepted => 1i8,
        GoalStatus::Executing => 2i8,
        GoalStatus::Canceling => 3i8,
        GoalStatus::Succeeded => 4i8,
        GoalStatus::Canceled => 5i8,
        GoalStatus::Aborted => 6i8,
    }
}

/// Spec: `GoalStatus::from_i8()` — maps discriminant back to variant.
///
/// Mirrors (action.rs:103-115):
/// ```ignore
/// pub fn from_i8(value: i8) -> Option<Self> {
///     match value { 0 => Some(Unknown), ..., 6 => Some(Aborted), _ => None }
/// }
/// ```
pub open spec fn from_i8_spec(v: i8) -> Option<GoalStatus> {
    if v == 0i8 { Some(GoalStatus::Unknown) }
    else if v == 1i8 { Some(GoalStatus::Accepted) }
    else if v == 2i8 { Some(GoalStatus::Executing) }
    else if v == 3i8 { Some(GoalStatus::Canceling) }
    else if v == 4i8 { Some(GoalStatus::Succeeded) }
    else if v == 5i8 { Some(GoalStatus::Canceled) }
    else if v == 6i8 { Some(GoalStatus::Aborted) }
    else { None }
}

/// Spec: valid state transitions in the action protocol.
///
/// Models the ROS 2 action lifecycle:
/// - Accepted → Executing, Canceling
/// - Executing → Succeeded, Aborted, Canceling
/// - Canceling → Canceled, Aborted
/// - Unknown, Succeeded, Canceled, Aborted → (no outgoing transitions)
pub open spec fn is_valid_transition(from: GoalStatus, to: GoalStatus) -> bool {
    match from {
        GoalStatus::Accepted =>
            to is Executing || to is Canceling,
        GoalStatus::Executing =>
            to is Succeeded || to is Aborted || to is Canceling,
        GoalStatus::Canceling =>
            to is Canceled || to is Aborted,
        _ => false,
    }
}

/// Ranking function for transition DAG proof.
///
/// Every valid transition strictly decreases this rank, proving the
/// transition graph is acyclic (a DAG).
pub open spec fn status_rank(s: GoalStatus) -> int {
    match s {
        GoalStatus::Unknown => 0,
        GoalStatus::Accepted => 3,
        GoalStatus::Executing => 2,
        GoalStatus::Canceling => 1,
        GoalStatus::Succeeded => 0,
        GoalStatus::Canceled => 0,
        GoalStatus::Aborted => 0,
    }
}

// ======================================================================
// Trusted Contracts
// ======================================================================

/// Trusted contract: `GoalStatus::is_terminal()` matches `is_terminal_spec`.
///
/// A human auditor should confirm the 3-variant match in `is_terminal_spec`
/// corresponds to the 3-variant `matches!` in action.rs:87-93.
pub assume_specification[ GoalStatus::is_terminal ](self_: &GoalStatus) -> (ret: bool)
    ensures
        ret == is_terminal_spec(*self_);

/// Trusted contract: `GoalStatus::is_active()` matches `is_active_spec`.
pub assume_specification[ GoalStatus::is_active ](self_: &GoalStatus) -> (ret: bool)
    ensures
        ret == is_active_spec(*self_);

/// Trusted contract: `GoalStatus::from_i8()` matches `from_i8_spec`.
pub assume_specification[ GoalStatus::from_i8 ](value: i8) -> (ret: Option<GoalStatus>)
    ensures
        ret == from_i8_spec(value);

// ======================================================================
// Proofs
// ======================================================================

/// **Proof: `terminal_active_disjoint`**
///
/// No GoalStatus variant is both terminal and active. These are disjoint
/// categories: terminal = {Succeeded, Canceled, Aborted},
/// active = {Accepted, Executing, Canceling}, and Unknown is neither.
///
/// Real-time relevance: Application code can safely branch on is_terminal()
/// vs is_active() without ambiguity.
proof fn terminal_active_disjoint(s: GoalStatus)
    ensures
        !(is_terminal_spec(s) && is_active_spec(s)),
{
    // Z3 enumerates all 7 variants and checks the disjointness
}

/// **Proof: `valid_status_exhaustive`**
///
/// `from_i8_spec` maps each discriminant value (0-6) to the correct variant.
/// This proves the mapping is exhaustive — every valid GoalStatus has a
/// corresponding i8 value.
///
/// Real-time relevance: CDR deserialization of goal status codes is complete.
proof fn valid_status_exhaustive()
    ensures
        from_i8_spec(0i8) == Some(GoalStatus::Unknown),
        from_i8_spec(1i8) == Some(GoalStatus::Accepted),
        from_i8_spec(2i8) == Some(GoalStatus::Executing),
        from_i8_spec(3i8) == Some(GoalStatus::Canceling),
        from_i8_spec(4i8) == Some(GoalStatus::Succeeded),
        from_i8_spec(5i8) == Some(GoalStatus::Canceled),
        from_i8_spec(6i8) == Some(GoalStatus::Aborted),
        // Out-of-range values return None
        from_i8_spec(7i8).is_none(),
        from_i8_spec(-1i8).is_none(),
{
}

/// **Proof: `transition_validity`**
///
/// Valid GoalStatus transitions form a DAG — no cycles are possible.
/// Proved via a ranking function: every valid transition strictly decreases
/// `status_rank`, and since ranks are non-negative integers, cycles are
/// impossible (rank can't decrease indefinitely).
///
/// Ranking: Accepted(3) → Executing(2) → Canceling(1) → terminal(0)
///
/// Real-time relevance: Goal state machines always make progress toward
/// completion. No infinite loops in the action lifecycle.
proof fn transition_validity(from: GoalStatus, to: GoalStatus)
    requires
        is_valid_transition(from, to),
    ensures
        // Every valid transition strictly decreases rank
        status_rank(to) < status_rank(from),
        // Terminal states have rank 0 (no outgoing transitions)
        status_rank(to) >= 0,
{
}

/// **Proof: `from_i8_roundtrip`**
///
/// For every GoalStatus variant, converting to i8 and back via from_i8
/// recovers the original variant: `from_i8(status as i8) == Some(status)`.
///
/// Real-time relevance: CDR serialization (write as i8, read back) preserves
/// the exact goal status.
proof fn from_i8_roundtrip(s: GoalStatus)
    ensures
        from_i8_spec(to_i8_spec(s)) == Some(s),
{
    // Z3 enumerates all 7 variants and checks the roundtrip
}

} // verus!
