//! Phase 110.F — `OsPrioritySet` (per-priority OS-thread dispatch).
//!
//! **Status:** stub + reserved namespace. Dispatch model intentionally
//! left unspecified pending the future **node-orchestration phase**,
//! which will define the canonical mapping from callback / chain
//! identity to OS priority. nano-ros may not adopt PiCAS as written —
//! the orchestration phase will pick the actual approach (PiCAS,
//! per-SC priority, chain-derived priority, or something else).
//!
//! ## Shape that's locked now
//!
//! Phase 110.A–E share one Executor thread (or one per
//! `open_threaded` call) and dispatch cooperatively from a
//! `ReadySet`. The `scheduler-os-priority` feature gate carves out
//! a slot for a future model where dispatch crosses thread
//! boundaries — workers keyed by OS priority, callbacks dispatched
//! to the worker matching their bound priority.
//!
//! ## Cross-cutting concerns the orchestration phase must address
//!
//! Independent of which exact model lands, the executor side will
//! need:
//!
//! 1. **Callback closures `Send + 'static`** for any cross-thread
//!    dispatch path. Current `add_subscription<F>` already requires
//!    `F: FnMut(&M) + Send + 'static` for std workloads, so this
//!    side is mostly settled.
//! 2. **Per-`DescIdx` exclusive arena access.** Each entry's arena
//!    slot is touched by at most one worker → `unsafe impl Send`
//!    with a documented invariant covers it; no per-entry mutex
//!    needed.
//! 3. **Worker-pool lifecycle.** Spawn lazily on first opt-in;
//!    halt + join in `Drop for Executor`.
//! 4. **Worker self-elevation** via
//!    `PlatformScheduler::set_current_thread_policy` at startup.
//! 5. **Trigger-eval consistency** — cross-thread dispatch races
//!    against the next `spin_once` cycle's activator scan.
//!
//! Reference reading: PiCAS (RTAS '21), CIL-EDF, HSE. None are
//! adopted prescriptively.

#![cfg(feature = "scheduler-os-priority")]

use super::super::types::{ActiveJob, DescIdx};
use super::{Overflow, ReadySet};

/// Stub — see module docs. Cross-thread per-priority dispatch lives
/// outside the `ReadySet` abstraction (worker pool + mailboxes), so
/// this type currently delegates to [`super::FifoReadySet`]
/// semantics. Real impl shape is intentionally deferred to the
/// future node-orchestration phase.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub(crate) struct OsPrioritySet<const N: usize> {
    inner: super::FifoReadySet<N>,
}

#[allow(dead_code)]
impl<const N: usize> OsPrioritySet<N> {
    pub const fn new() -> Self {
        Self {
            inner: super::FifoReadySet::<N>::new(),
        }
    }
}

impl<const N: usize> ReadySet for OsPrioritySet<N> {
    fn clear(&mut self) {
        self.inner.clear()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn insert(&mut self, job: ActiveJob) -> Result<(), Overflow> {
        self.inner.insert(job)
    }

    fn pop_next(&mut self) -> Option<ActiveJob> {
        self.inner.pop_next()
    }

    fn contains(&self, desc_idx: DescIdx) -> bool {
        self.inner.contains(desc_idx)
    }
}
