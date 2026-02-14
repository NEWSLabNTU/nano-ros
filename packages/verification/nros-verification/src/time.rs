/// Duration/Time arithmetic proofs (Phase 31.2 smoke tests + Phase 31.4 proofs)
///
/// Proves correctness of nanosecond decomposition, round-trip integrity, and
/// time arithmetic identities that underpin the executor's scheduling logic.
///
/// ## Trust levels
///
/// **Formally linked** (via `assume_specification` + `external_type_specification`):
/// - `Duration` — transparent type, pub fields (sec: i32, nanosec: u32).
/// - `Time` — transparent type, pub fields (sec: i32, nanosec: u32).
/// - `Duration::from_nanos` — linked to postconditions about field decomposition.
/// - `Duration::to_nanos` — linked to postconditions about recomposition.
///
/// **Pure math** (no link to production code):
/// - Round-trip identity, ordering consistency, add/sub inverse.
/// - `time_from_nanos_bug` — proves failure domain for Time::from_nanos
///   (which omits `.unsigned_abs()` on the remainder for negative inputs).
use vstd::prelude::*;
use nros_core::time::{Duration, Time};

verus! {

// ======================================================================
// Type Specifications
// ======================================================================

/// Register `Duration` with Verus as a transparent type.
/// Pub fields: `sec: i32`, `nanosec: u32`.
#[verifier::external_type_specification]
pub struct ExDuration(nros_core::time::Duration);

/// Register `Time` with Verus as a transparent type.
/// Pub fields: `sec: i32`, `nanosec: u32` (same layout as Duration).
#[verifier::external_type_specification]
pub struct ExTime(nros_core::time::Time);

// ======================================================================
// Trusted Contracts
// ======================================================================

/// Trusted contract: `Duration::from_nanos` decomposes nanoseconds into sec + nanosec.
///
/// Source (time.rs:174-178):
/// ```ignore
/// pub const fn from_nanos(nanos: i64) -> Self {
///     let sec = (nanos / NANOS_PER_SEC) as i32;
///     let nanosec = (nanos % NANOS_PER_SEC).unsigned_abs() as u32;
///     Self { sec, nanosec }
/// }
/// ```
///
/// The nanosec component is always in [0, 1_000_000_000).
/// For non-negative nanos, nanosec equals nanos % 1e9 exactly.
pub assume_specification[ Duration::from_nanos ](nanos: i64) -> (d: Duration)
    ensures
        0 <= d.nanosec < 1_000_000_000,
        d.sec == ((nanos / 1_000_000_000) as i32),
        // For non-negative nanos, unsigned_abs() is identity, so nanosec == nanos % 1e9
        nanos >= 0 ==> d.nanosec as int == nanos % 1_000_000_000;

/// Trusted contract: `Duration::to_nanos` recomposes to total nanoseconds.
///
/// Source (time.rs:181-183):
/// ```ignore
/// pub const fn to_nanos(&self) -> i64 {
///     (self.sec as i64) * NANOS_PER_SEC + (self.nanosec as i64)
/// }
/// ```
pub assume_specification[ Duration::to_nanos ](self_: &Duration) -> (n: i64)
    ensures
        n == (self_.sec as i64) * 1_000_000_000i64 + (self_.nanosec as i64);

// ======================================================================
// Existing Proofs (Phase 31.2)
// ======================================================================

/// Smoke test 1: prove integer remainder bound used by Duration::from_nanos.
/// Z3 proves: for all i64, |n % 1_000_000_000| < 1_000_000_000.
proof fn remainder_bounded(n: i64)
    ensures
        -1_000_000_000 < n % 1_000_000_000 < 1_000_000_000,
{
}

/// Smoke test 2: prove Duration field access works and to_nanos has bounded output
/// for any Duration satisfying the nanosec invariant.
proof fn duration_to_nanos_bounded(d: Duration)
    requires
        0 <= d.nanosec < 1_000_000_000,
    ensures
        (d.sec as i64) * 1_000_000_000 + (d.nanosec as i64) <= i32::MAX as i64 * 1_000_000_000 + 999_999_999,
        (d.sec as i64) * 1_000_000_000 + (d.nanosec as i64) >= i32::MIN as i64 * 1_000_000_000,
{
}

// ======================================================================
// Phase 31.4 Proofs: Duration/Time Arithmetic
// ======================================================================

/// **Proof: `duration_from_nanos_roundtrip`**
///
/// For non-negative nanoseconds within Duration range, the Euclidean division
/// identity holds: `(n / 1e9) * 1e9 + (n % 1e9) == n`.
///
/// Combined with the assume_specifications:
/// - `from_nanos(n)` sets sec = n/1e9, nanosec = n%1e9 (non-negative case)
/// - `to_nanos(d)` returns sec*1e9 + nanosec
/// → `to_nanos(from_nanos(n)) == n` for non-negative n in range.
///
/// Real-time relevance: Duration round-trip through CDR serialization is lossless.
proof fn duration_from_nanos_roundtrip(nanos: i64)
    requires
        nanos >= 0,
        // sec = nanos/1e9 must fit in i32 (no truncation in the `as i32` cast)
        nanos / 1_000_000_000 <= i32::MAX as i64,
    ensures
        // Euclidean division identity: the foundation of the round-trip
        (nanos / 1_000_000_000) * 1_000_000_000 + nanos % 1_000_000_000 == nanos,
{
}

/// **Proof: `duration_components_valid`**
///
/// For any non-negative nanosecond input, the remainder is a valid nanosec
/// component: `0 <= n % 1e9 < 1e9`.
///
/// This is the mathematical basis for the from_nanos spec's first ensures clause.
/// Combined with the assume_specification, it guarantees that from_nanos always
/// produces a Duration with valid nanosec, for any non-negative input.
///
/// Real-time relevance: Duration values always satisfy their invariant.
proof fn duration_components_valid(nanos: i64)
    requires
        nanos >= 0,
    ensures
        0 <= nanos % 1_000_000_000 < 1_000_000_000,
{
}

/// **Proof: `time_add_sub_inverse`**
///
/// Addition and subtraction are exact inverses for non-negative time values:
/// `from_nanos(t + d - d) == from_nanos(t)` — the sec and nanosec components
/// are identical after add-then-subtract.
///
/// This models the Time arithmetic:
/// ```ignore
/// fn add(self, rhs: Duration) -> Time {
///     Time::from_nanos(self.to_nanos() + rhs.to_nanos())
/// }
/// fn sub(self, rhs: Duration) -> Time {
///     Time::from_nanos(self.to_nanos() - rhs.to_nanos())
/// }
/// ```
///
/// The proof establishes that integer arithmetic has no drift — unlike
/// floating-point, `(t + d) - d` recovers `t` exactly.
///
/// Real-time relevance: Time arithmetic in scheduling is exact, no cumulative error.
proof fn time_add_sub_inverse(t_nanos: i64, d_nanos: i64)
    requires
        t_nanos >= 0,
        d_nanos >= 0,
        t_nanos as int + d_nanos as int <= i64::MAX as int,
    ensures
        // Nanos-level cancellation
        t_nanos + d_nanos - d_nanos == t_nanos,
        // Component-level cancellation (sec field)
        (t_nanos + d_nanos - d_nanos) / 1_000_000_000 == t_nanos / 1_000_000_000,
        // Component-level cancellation (nanosec field)
        (t_nanos + d_nanos - d_nanos) % 1_000_000_000 == t_nanos % 1_000_000_000,
{
}

/// **Proof: `time_ordering_consistent`**
///
/// Lexicographic ordering on `(sec, nanosec)` is equivalent to total nanoseconds
/// ordering, provided both nanosec components are in the valid range [0, 1e9).
///
/// Rust's derived `PartialOrd`/`Ord` on Time and Duration uses lexicographic
/// ordering on struct fields. This proof shows it matches the total order on
/// the mathematical nanosecond representation.
///
/// Real-time relevance: Timer comparisons (`elapsed >= period`) are correct
/// whether done on fields or on total nanoseconds.
proof fn time_ordering_consistent(
    sec1: i32, ns1: u32,
    sec2: i32, ns2: u32,
)
    requires
        0 <= ns1 < 1_000_000_000u32,
        0 <= ns2 < 1_000_000_000u32,
    ensures
        // Lexicographic < iff total nanos <
        // Uses `as int` (Verus spec arithmetic — unbounded integers) to avoid
        // i64 overflow concerns in the proof.
        (sec1 < sec2 || (sec1 == sec2 && ns1 < ns2))
        <==>
        (sec1 as int * 1_000_000_000 + ns1 as int) < (sec2 as int * 1_000_000_000 + ns2 as int),
{
    // Forward direction: lexicographic → total nanos
    // If sec1 < sec2: sec1*1e9 + ns1 <= sec1*1e9 + (1e9-1) < (sec1+1)*1e9 <= sec2*1e9 <= sec2*1e9+ns2
    // If sec1 == sec2 && ns1 < ns2: trivial

    // Reverse direction: total nanos → lexicographic
    // If nanos1 < nanos2 and sec1 > sec2: then sec1*1e9 >= (sec2+1)*1e9 = sec2*1e9 + 1e9
    //   and nanos1 = sec1*1e9 + ns1 >= sec2*1e9 + 1e9 > sec2*1e9 + ns2 = nanos2 — contradiction
    // If sec1 == sec2: nanos1 < nanos2 → ns1 < ns2

    // Help Z3 with the key nonlinear bounds
    if sec1 < sec2 {
        // sec1 + 1 <= sec2, so (sec1+1)*1e9 <= sec2*1e9
        assert((sec1 as int + 1) * 1_000_000_000 <= sec2 as int * 1_000_000_000)
            by (nonlinear_arith)
            requires sec1 as int + 1 <= sec2 as int;
    }
    if sec1 > sec2 {
        // sec2 + 1 <= sec1, so (sec2+1)*1e9 <= sec1*1e9
        assert(sec1 as int * 1_000_000_000 >= (sec2 as int + 1) * 1_000_000_000)
            by (nonlinear_arith)
            requires sec2 as int + 1 <= sec1 as int;
    }
}

/// **Proof: `time_from_nanos_bug`**
///
/// Demonstrates the bug in `Time::from_nanos` for negative nanoseconds.
/// `Time::from_nanos` does `(nanos % 1e9) as u32` WITHOUT `.unsigned_abs()`,
/// unlike `Duration::from_nanos` which correctly uses `.unsigned_abs()`.
///
/// For negative remainders, the `as u32` cast produces a value via two's
/// complement wrapping: `r as u32 = r + 2^32` for -2^32 < r < 0.
/// When -1e9 < r < 0, this gives `r + 2^32 > 3.29e9 >> 999_999_999`,
/// violating the nanosec < 1e9 invariant.
///
/// Source (time.rs:39-43):
/// ```ignore
/// pub const fn from_nanos(nanos: i64) -> Self {
///     let sec = (nanos / NANOS_PER_SEC) as i32;
///     let nanosec = (nanos % NANOS_PER_SEC) as u32;  // BUG: no .unsigned_abs()
///     Self { sec, nanosec }
/// }
/// ```
///
/// Real-time relevance: Negative Time values (before epoch) produce invalid
/// nanosec fields. Duration::from_nanos is correct; Time::from_nanos is not.
proof fn time_from_nanos_bug(r: int)
    requires
        // r models `nanos % 1_000_000_000` for negative nanos with non-zero remainder
        -1_000_000_000 < r,
        r < 0,
    ensures
        // The u32 representation (r + 2^32, since -2^32 < r < 0) exceeds valid nanosec range
        r + 4_294_967_296 > 999_999_999,
{
    // r > -1e9, so r + 2^32 > 2^32 - 1e9 = 3,294,967,296 > 999,999,999
}

} // verus!
