/// GoalStatusCode state machine proofs (Phase 31.4)
///
/// Proves correctness of the ROS 2 action GoalStatusCode lifecycle: terminal/active
/// disjointness, status code round-trip, and transition DAG structure.
///
/// ## Trust levels
///
/// **Formally linked** (via `assume_specification` + `external_type_specification`):
/// - `GoalStatusCode` — transparent enum registered via `external_type_specification`
///   (without `external_body`). Verus can match on all 7 variants directly.
/// - `GoalStatusCode::is_terminal()` — linked to `is_terminal_spec` via `assume_specification`.
/// - `GoalStatusCode::is_active()` — linked to `is_active_spec` via `assume_specification`.
/// - `GoalStatusCode::from_i8()` — linked to `from_i8_spec` via `assume_specification`.
///
/// **Pure math** (no link to production code):
/// - `transition_validity` — transition DAG property proved via ranking function.
use vstd::prelude::*;
use nros_core::GoalStatusCode;

verus! {

// ======================================================================
// GoalStatusCode Type Specification
// ======================================================================

/// Register `GoalStatusCode` with Verus as a transparent type.
///
/// Without `external_body`, Verus sees the enum's variant structure and allows
/// pattern matching on Unknown, Accepted, Executing, Canceling, Succeeded,
/// Canceled, Aborted.
///
/// Source (action.rs:62-84):
/// ```ignore
/// #[repr(i8)]
/// pub enum GoalStatusCode {
///     Unknown = 0, Accepted = 1, Executing = 2, Canceling = 3,
///     Succeeded = 4, Canceled = 5, Aborted = 6,
/// }
/// ```
#[verifier::external_type_specification]
pub struct ExGoalStatus(GoalStatusCode);

// ======================================================================
// Spec Functions
// ======================================================================

/// Spec: `GoalStatusCode::is_terminal()` — true for completed states.
///
/// Mirrors (action.rs:87-93):
/// ```ignore
/// pub fn is_terminal(&self) -> bool {
///     matches!(self, GoalStatusCode::Succeeded | GoalStatusCode::Canceled | GoalStatusCode::Aborted)
/// }
/// ```
pub open spec fn is_terminal_spec(s: GoalStatusCode) -> bool {
    match s {
        GoalStatusCode::Succeeded | GoalStatusCode::Canceled | GoalStatusCode::Aborted => true,
        _ => false,
    }
}

/// Spec: `GoalStatusCode::is_active()` — true for in-progress states.
///
/// Mirrors (action.rs:95-101):
/// ```ignore
/// pub fn is_active(&self) -> bool {
///     matches!(self, GoalStatusCode::Accepted | GoalStatusCode::Executing | GoalStatusCode::Canceling)
/// }
/// ```
pub open spec fn is_active_spec(s: GoalStatusCode) -> bool {
    match s {
        GoalStatusCode::Accepted | GoalStatusCode::Executing | GoalStatusCode::Canceling => true,
        _ => false,
    }
}

/// Spec: `GoalStatusCode as i8` — the repr(i8) discriminant.
///
/// Mirrors the `#[repr(i8)]` discriminant values on the enum variants.
pub open spec fn to_i8_spec(s: GoalStatusCode) -> i8 {
    match s {
        GoalStatusCode::Unknown => 0i8,
        GoalStatusCode::Accepted => 1i8,
        GoalStatusCode::Executing => 2i8,
        GoalStatusCode::Canceling => 3i8,
        GoalStatusCode::Succeeded => 4i8,
        GoalStatusCode::Canceled => 5i8,
        GoalStatusCode::Aborted => 6i8,
    }
}

/// Spec: `GoalStatusCode::from_i8()` — maps discriminant back to variant.
///
/// Mirrors (action.rs:103-115):
/// ```ignore
/// pub fn from_i8(value: i8) -> Option<Self> {
///     match value { 0 => Some(Unknown), ..., 6 => Some(Aborted), _ => None }
/// }
/// ```
pub open spec fn from_i8_spec(v: i8) -> Option<GoalStatusCode> {
    if v == 0i8 { Some(GoalStatusCode::Unknown) }
    else if v == 1i8 { Some(GoalStatusCode::Accepted) }
    else if v == 2i8 { Some(GoalStatusCode::Executing) }
    else if v == 3i8 { Some(GoalStatusCode::Canceling) }
    else if v == 4i8 { Some(GoalStatusCode::Succeeded) }
    else if v == 5i8 { Some(GoalStatusCode::Canceled) }
    else if v == 6i8 { Some(GoalStatusCode::Aborted) }
    else { None }
}

/// Spec: valid state transitions in the action protocol.
///
/// Models the ROS 2 action lifecycle:
/// - Accepted → Executing, Canceling
/// - Executing → Succeeded, Aborted, Canceling
/// - Canceling → Canceled, Aborted
/// - Unknown, Succeeded, Canceled, Aborted → (no outgoing transitions)
pub open spec fn is_valid_transition(from: GoalStatusCode, to: GoalStatusCode) -> bool {
    match from {
        GoalStatusCode::Accepted =>
            to is Executing || to is Canceling,
        GoalStatusCode::Executing =>
            to is Succeeded || to is Aborted || to is Canceling,
        GoalStatusCode::Canceling =>
            to is Canceled || to is Aborted,
        _ => false,
    }
}

/// Ranking function for transition DAG proof.
///
/// Every valid transition strictly decreases this rank, proving the
/// transition graph is acyclic (a DAG).
pub open spec fn status_rank(s: GoalStatusCode) -> int {
    match s {
        GoalStatusCode::Unknown => 0,
        GoalStatusCode::Accepted => 3,
        GoalStatusCode::Executing => 2,
        GoalStatusCode::Canceling => 1,
        GoalStatusCode::Succeeded => 0,
        GoalStatusCode::Canceled => 0,
        GoalStatusCode::Aborted => 0,
    }
}

// ======================================================================
// Trusted Contracts
// ======================================================================

/// Trusted contract: `GoalStatusCode::is_terminal()` matches `is_terminal_spec`.
///
/// A human auditor should confirm the 3-variant match in `is_terminal_spec`
/// corresponds to the 3-variant `matches!` in action.rs:87-93.
pub assume_specification[ GoalStatusCode::is_terminal ](self_: &GoalStatusCode) -> (ret: bool)
    ensures
        ret == is_terminal_spec(*self_);

/// Trusted contract: `GoalStatusCode::is_active()` matches `is_active_spec`.
pub assume_specification[ GoalStatusCode::is_active ](self_: &GoalStatusCode) -> (ret: bool)
    ensures
        ret == is_active_spec(*self_);

/// Trusted contract: `GoalStatusCode::from_i8()` matches `from_i8_spec`.
pub assume_specification[ GoalStatusCode::from_i8 ](value: i8) -> (ret: Option<GoalStatusCode>)
    ensures
        ret == from_i8_spec(value);

// ======================================================================
// Proofs
// ======================================================================

/// **Proof: `terminal_active_disjoint`**
///
/// No GoalStatusCode variant is both terminal and active. These are disjoint
/// categories: terminal = {Succeeded, Canceled, Aborted},
/// active = {Accepted, Executing, Canceling}, and Unknown is neither.
///
/// Real-time relevance: Application code can safely branch on is_terminal()
/// vs is_active() without ambiguity.
proof fn terminal_active_disjoint(s: GoalStatusCode)
    ensures
        !(is_terminal_spec(s) && is_active_spec(s)),
{
    // Z3 enumerates all 7 variants and checks the disjointness
}

/// **Proof: `valid_status_exhaustive`**
///
/// `from_i8_spec` maps each discriminant value (0-6) to the correct variant.
/// This proves the mapping is exhaustive — every valid GoalStatusCode has a
/// corresponding i8 value.
///
/// Real-time relevance: CDR deserialization of goal status codes is complete.
proof fn valid_status_exhaustive()
    ensures
        from_i8_spec(0i8) == Some(GoalStatusCode::Unknown),
        from_i8_spec(1i8) == Some(GoalStatusCode::Accepted),
        from_i8_spec(2i8) == Some(GoalStatusCode::Executing),
        from_i8_spec(3i8) == Some(GoalStatusCode::Canceling),
        from_i8_spec(4i8) == Some(GoalStatusCode::Succeeded),
        from_i8_spec(5i8) == Some(GoalStatusCode::Canceled),
        from_i8_spec(6i8) == Some(GoalStatusCode::Aborted),
        // Out-of-range values return None
        from_i8_spec(7i8).is_none(),
        from_i8_spec(-1i8).is_none(),
{
}

/// **Proof: `transition_validity`**
///
/// Valid GoalStatusCode transitions form a DAG — no cycles are possible.
/// Proved via a ranking function: every valid transition strictly decreases
/// `status_rank`, and since ranks are non-negative integers, cycles are
/// impossible (rank can't decrease indefinitely).
///
/// Ranking: Accepted(3) → Executing(2) → Canceling(1) → terminal(0)
///
/// Real-time relevance: Goal state machines always make progress toward
/// completion. No infinite loops in the action lifecycle.
proof fn transition_validity(from: GoalStatusCode, to: GoalStatusCode)
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
/// For every GoalStatusCode variant, converting to i8 and back via from_i8
/// recovers the original variant: `from_i8(status as i8) == Some(status)`.
///
/// Real-time relevance: CDR serialization (write as i8, read back) preserves
/// the exact goal status.
proof fn from_i8_roundtrip(s: GoalStatusCode)
    ensures
        from_i8_spec(to_i8_spec(s)) == Some(s),
{
    // Z3 enumerates all 7 variants and checks the roundtrip
}

} // verus!
