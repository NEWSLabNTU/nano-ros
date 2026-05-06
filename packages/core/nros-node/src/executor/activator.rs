//! Phase 110.A — `Activator` trait.
//!
//! The activator decides *what* should fire this cycle. It is invoked
//! after `Session::drive_io` returns, walks the executor's `entries[]`
//! looking at `meta.has_data`, evaluates the configured `Trigger`,
//! and writes the resulting jobs into a [`ReadySet`](super::ready_set::ReadySet).
//!
//! 110.A defines the trait; 110.A.b rewires `spin_once` to drive
//! activation through this trait instead of the inline bitmap scan.
//! The default impl reproduces the pre-refactor scan + trigger logic
//! bit-for-bit.

use super::ready_set::ReadySet;

/// Context handed to [`Activator::scan`]. Phase 110.A keeps this
/// opaque — the default activator impl borrows the executor's
/// `entries[]`, `arena_ptr`, and `trigger` directly. 110.A.b widens
/// this once the inline scan moves into the trait method.
#[allow(dead_code)] // Phase 110.A — wired in 110.A.b spin_once rewire.
pub(crate) struct ActivatorCtx<'a> {
    _phantom: core::marker::PhantomData<&'a ()>,
}

#[allow(dead_code)] // Phase 110.A — wired in 110.A.b spin_once rewire.
pub(crate) trait Activator {
    /// Walk the executor's entries, evaluate the trigger, and insert
    /// every callback that should fire into `ready`. Idempotent on
    /// already-set entries (`ReadySet::insert` is itself idempotent).
    fn scan<R: ReadySet>(&self, ctx: &ActivatorCtx<'_>, ready: &mut R);
}
