//! Phase 110.F — `OsPrioritySet` (PiCAS-style per-callback OS priority).
//!
//! **Status:** stub. Type surface locked; dispatch impl deferred.
//!
//! ## What PiCAS does
//!
//! The PiCAS Algorithm 1 (RTAS '21) burns one OS priority slot per
//! `(callback × chain)`: every callback dispatches on a thread that
//! the OS scheduler runs at that callback's bound priority. Cross-
//! callback preemption falls out of OS scheduling for free — no
//! cooperative ready-set required.
//!
//! ## Why this is a different dispatch model
//!
//! Phase 110.A–E share one Executor thread (or one per `open_threaded`
//! call) and dispatch callbacks cooperatively from a `ReadySet`.
//! `OsPrioritySet` instead maintains a thread pool keyed by OS
//! priority: each entry's bound `SchedContext.os_pri` (when the SC's
//! class is `Fifo`-like) selects the worker thread to dispatch on.
//!
//! ## What's needed for a real impl
//!
//! 1. **Callback closures `Send + 'static`.** Today's `add_subscription`
//!    accepts `FnMut` closures that may capture non-Send references
//!    (typical for executor-local state). PiCAS workers run on a
//!    different thread than `spin_once` so the closures must move.
//!    Either reshape the public `add_*` API to require `Send` (mild
//!    breakage) or restrict `OsPrioritySet` to a separate
//!    `add_*_picas` constructor surface.
//! 2. **Arena `Send`-shareable.** `Executor.arena` is a stack-
//!    allocated `[MaybeUninit<u8>]`; workers reading from it across
//!    threads need `&Arena: Send` plus interior synchronisation to
//!    avoid races on entry buffers. Easiest path: per-entry `Mutex`
//!    or move to `Box<[u8]>` shared via `Arc`.
//! 3. **Worker pool lifecycle.** Spawn one worker per distinct
//!    `os_pri` observed across registered SCs; `Drop for Executor`
//!    halts + joins all workers.
//! 4. **Per-worker mailbox.** SPSC channel from `spin_once` to each
//!    worker; activator scan dispatches `DescIdx` into the matching
//!    priority slot.
//! 5. **PlatformScheduler call from worker startup.** Each worker
//!    sets its own thread priority via
//!    `PlatformScheduler::set_current_thread_policy(SchedPolicy::Fifo
//!    { os_pri })` before draining its mailbox.
//!
//! Until those land, `OsPrioritySet` is reserved nomenclature only.
//! Enabling `feature = "scheduler-os-priority"` compiles this module
//! but doesn't change dispatch behavior.

#![cfg(feature = "scheduler-os-priority")]

use super::super::types::{ActiveJob, DescIdx};
use super::{Overflow, ReadySet};

/// Stub — see module docs. The actual PiCAS dispatch model lives
/// outside the `ReadySet` abstraction (worker threads + mailboxes,
/// not a single in-process queue), so this type currently delegates
/// to [`super::FifoReadySet`] semantics. Real impl ships once the
/// callback-Send + arena-share constraints from the module docs are
/// solved.
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
