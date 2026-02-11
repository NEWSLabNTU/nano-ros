use vstd::prelude::*;
use nano_ros_core::time::Duration;

verus! {

/// External type specification: tells Verus about Duration from nano-ros-core.
#[verifier::external_type_specification]
pub struct ExDuration(nano_ros_core::time::Duration);

/// Trusted contract: `Duration::from_nanos` decomposes nanoseconds into sec + nanosec.
/// The nanosec component is always in [0, 1_000_000_000).
pub assume_specification[ Duration::from_nanos ](nanos: i64) -> (d: Duration)
    ensures
        0 <= d.nanosec < 1_000_000_000,
        d.sec == ((nanos / 1_000_000_000) as i32);

/// Trusted contract: `Duration::to_nanos` recomposes to total nanoseconds.
pub assume_specification[ Duration::to_nanos ](self_: &Duration) -> (n: i64)
    ensures
        n == (self_.sec as i64) * 1_000_000_000i64 + (self_.nanosec as i64);

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

} // verus!
