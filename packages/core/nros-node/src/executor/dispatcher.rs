//! Phase 110.A — `Dispatcher` trait.
//!
//! The dispatcher consumes a [`ReadySet`](super::ready_set::ReadySet)
//! produced by [`Activator`](super::activator::Activator) and fires
//! the corresponding callbacks. It binds the arena pointer at
//! construction time (no raw `*mut u8` per call) and respects the
//! configured [`DrainMode`](super::types::DrainMode) — `Latched`
//! drains the snapshot only, `Greedy` re-runs the activator after
//! each callback.
//!
//! 110.A defines the trait; 110.A.b rewires `spin_once` to drive
//! dispatch through it. Default impl reproduces the pre-refactor
//! `try_process` loop bit-for-bit.

use super::ready_set::ReadySet;
use super::types::SpinOnceResult;

#[allow(dead_code)] // Phase 110.A — wired in 110.A.b spin_once rewire.
pub(crate) trait Dispatcher {
    /// Drain `ready` and fire each callback. Returns aggregate counts
    /// for the cycle.
    fn dispatch<R: ReadySet>(&mut self, ready: &mut R, delta_ms: u64) -> SpinOnceResult;
}
