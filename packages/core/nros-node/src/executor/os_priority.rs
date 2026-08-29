//! Phase 110.F — per-callback OS priority worker pool.
//!
//! One worker task per distinct non-zero `SchedContext.os_pri`. Each
//! self-elevates through the executor's stored `apply_policy` fn pointer at
//! startup, then drains a bounded mailbox of [`WorkItem`]s. Entries bound to an
//! SC with `os_pri > 0` dispatch here instead of on the cooperative path, so the
//! OS scheduler — not the executor — decides when they run relative to each
//! other.
//!
//! ## phase-359 W10 — why this is not `std::thread` any more
//!
//! The original pool was `std::thread` + `std::sync::mpsc` + `HashMap`, which
//! made a capability of the CORE executor available only on `std` platforms —
//! and `std` is what this campaign removes. Every piece has a portable
//! equivalent that the platform layer already exports, so the pool now runs
//! everywhere rather than being deleted or moved out of reach:
//!
//! | was | is |
//! | --- | --- |
//! | `std::thread::Builder::spawn` | `nros_platform_task_init` |
//! | `JoinHandle::join` | `nros_platform_task_join` |
//! | `std::sync::mpsc::channel` | [`heapless::mpmc::MpMcQueue`] + [`NodeWake`] |
//! | `rx.recv_timeout(10ms)` | `NodeWake::wait_ms(10)` + `dequeue` |
//! | `HashMap<u8, Worker>` | [`heapless::FnvIndexMap`] |
//!
//! Three consequences are real and deliberate, not incidental:
//!
//! * **The mailbox is bounded.** `mpsc` was unbounded, so a producer outrunning
//!   a worker grew the queue without limit. Here `try_dispatch` returns `false`
//!   when full and the caller falls back to the cooperative path — backpressure
//!   instead of unbounded memory on the RT path.
//! * **The pool is capacity-limited** ([`MAX_PRIORITY_LEVELS`]). A distinct
//!   `os_pri` beyond that falls back rather than allocating.
//! * **A platform with no wake primitive gets no pool.** The worker's blocking
//!   wait IS the wake primitive; without one the loop could only spin. Those
//!   platforms fall back to the cooperative path, which is what they did before
//!   this feature existed.

// The feature half of this module's gate; the availability half is the
// `#[cfg(any(has_rmw, test))]` on its `mod` declaration.
//
// `rmw-cffi` is here because `node_wake` — which this module blocks on — has
// the same inner gate, and for the same reason: both call `nros_platform_*`
// symbols, which only a build that links a platform provider can resolve. That
// is a REAL dependency the `std::thread` pool did not have (a std thread needs
// no platform), and it is the honest one: a worker cannot be given an OS
// priority without an OS. Builds without a platform fall back to cooperative
// dispatch, which is what an `os_pri`-bound entry got before this feature.
#![cfg(all(
    feature = "alloc",
    feature = "rmw-cffi",
    feature = "scheduler-os-priority"
))]

use core::ffi::c_void;

use heapless::{FnvIndexMap, mpmc::MpMcQueue};
use portable_atomic::{AtomicBool, Ordering};
use portable_atomic_util::Arc;

use nros_platform_api::task::PlatformTask;

use super::node_wake::NodeWake;

/// Mailbox depth per worker. Power of two — `MpMcQueue` requires it.
///
/// Sized for burst tolerance, not throughput: the spin loop enqueues at most
/// one item per ready entry per cycle, and a worker that cannot keep up should
/// apply backpressure rather than buffer indefinitely (see the module docs).
const MAILBOX_DEPTH: usize = 16;

/// Stack for a worker task, in bytes.
///
/// phase-364 W3 — stated rather than defaulted. The workers run user callbacks
/// through `try_process`, so they need more than a port's minimal default; 16
/// KiB matches what the RTOS glue uses for a tier that carries the
/// zenoh-pico/executor call depth with its arena on the heap.
const WORKER_STACK_BYTES: usize = 16384;

/// Distinct non-zero `os_pri` values the pool can serve. Power of two —
/// `FnvIndexMap` requires it. PiCAS-style assignments use a handful of levels;
/// exceeding this falls back to cooperative dispatch rather than failing.
pub(crate) const MAX_PRIORITY_LEVELS: usize = 8;

/// One dispatch handed to a worker.
///
/// `arena_base` + `arena_offset` rather than a pointer so the item is plainly
/// `Send`: the executor's arena outlives every worker (see [`OsPriorityWorker`]
/// `Drop`), and the worker reconstitutes the address on the far side.
#[derive(Clone, Copy)]
pub(crate) struct WorkItem {
    pub(crate) arena_base: usize,
    pub(crate) arena_offset: usize,
    pub(crate) try_process: unsafe fn(*mut u8, u64, u8) -> Result<bool, nros_rmw::TransportError>,
    pub(crate) delta_us: u64,
    /// The entry's slot index, carried so the leaf callback hooks can name
    /// the callback they bracket (phase 8,
    /// `docs/design/callback_tracing.rst`). This path is the reason those
    /// hooks had to be thread-safe from day one: it runs `try_process` on a
    /// WORKER task, so two callbacks can legitimately be in flight at once
    /// and their events interleave in the capture. Keying every event on the
    /// handle — rather than holding an open span in a single slot — is what
    /// lets the decoder pair them anyway.
    pub(crate) desc_idx: u8,
}

// SAFETY: Phase 110.F per-DescIdx exclusive-access invariant — the activator
// scan in `spin_once` only sends a `WorkItem` for a given `arena_offset` to one
// worker per cycle, and won't re-send the same offset until the worker drains
// the previous one (`os_pri` dispatch is the worker's exclusive path;
// cooperative dispatch is skipped for SCs with non-zero `os_pri`). The fn
// pointer is Send-clean.
unsafe impl Send for WorkItem {}

/// State shared between the spin loop and one worker task.
///
/// Reached from the task through a raw pointer, so it must outlive the task —
/// [`OsPriorityWorker::drop`] joins before releasing its `Arc`.
struct WorkerCtx {
    mailbox: MpMcQueue<WorkItem, MAILBOX_DEPTH>,
    halt: AtomicBool,
    /// The worker's blocking wait, and the producer's doorbell.
    wake: NodeWake,
    apply_policy: fn(nros_platform_api::SchedPolicy) -> Result<(), nros_platform_api::SchedError>,
    os_pri: u8,
}

/// Task entry. Elevates, then drains until halted.
///
/// # Safety
/// `arg` must be a pointer to a live `WorkerCtx` that outlives this task.
unsafe extern "C" fn worker_entry(arg: *mut c_void) -> *mut c_void {
    // SAFETY: the spawn site passes `Arc::as_ptr` of a `WorkerCtx` it keeps
    // alive until after `task_join`.
    let ctx = unsafe { &*(arg as *const WorkerCtx) };

    // Self-elevate. Failure is not fatal: running at the default priority is
    // still correct, just without the guarantee — same contract as before.
    let _ = (ctx.apply_policy)(nros_platform_api::SchedPolicy::Fifo { os_pri: ctx.os_pri });

    while !ctx.halt.load(Ordering::Acquire) {
        // Drain everything queued before sleeping again, so a burst costs one
        // wait rather than one per item.
        while let Some(item) = ctx.mailbox.dequeue() {
            // SAFETY: `arena_base + arena_offset` addresses the executor's
            // arena, which outlives this task — `Executor::drop` halts and
            // JOINS every worker before the arena is released.
            let data = (item.arena_base as *mut u8).wrapping_add(item.arena_offset);
            let _ = unsafe { (item.try_process)(data, item.delta_us, item.desc_idx) };
        }
        // Bounded wait: the producer signals after every enqueue, and the
        // timeout bounds how long a halt takes to observe.
        let _ = ctx.wake.wait_ms(10);
    }
    core::ptr::null_mut()
}

/// One worker task, its mailbox, and the storage the platform needs to track it.
pub(crate) struct OsPriorityWorker {
    ctx: Arc<WorkerCtx>,
    task: Option<PlatformTask>,
}

impl OsPriorityWorker {
    /// Spawn a worker for `os_pri`, or `None` when this platform cannot host
    /// one (no wake primitive, no task storage, or the spawn was refused).
    ///
    /// `None` is not an error path the caller must handle specially: the
    /// dispatch site falls back to cooperative dispatch, which is what an entry
    /// with no worker got before this feature existed.
    pub(crate) fn spawn(
        os_pri: u8,
        apply_policy: fn(
            nros_platform_api::SchedPolicy,
        ) -> Result<(), nros_platform_api::SchedError>,
    ) -> Option<Self> {
        let wake = NodeWake::new()?;
        let ctx = Arc::new(WorkerCtx {
            mailbox: MpMcQueue::new(),
            halt: AtomicBool::new(false),
            wake,
            apply_policy,
            os_pri,
        });
        let arg = Arc::as_ptr(&ctx) as *mut c_void;
        // SAFETY: `arg` points at a `WorkerCtx` this struct owns and keeps
        // alive until after the join in `drop`. The name is a NUL-terminated
        // literal with static lifetime.
        let task = unsafe {
            PlatformTask::spawn(
                worker_entry,
                arg,
                WORKER_STACK_BYTES,
                c"nros-os-pri".as_ptr(),
            )
        }?;
        Some(Self {
            ctx,
            task: Some(task),
        })
    }

    /// Queue one dispatch. `false` when the mailbox is full — the caller then
    /// runs the entry cooperatively rather than dropping it.
    pub(crate) fn try_dispatch(&self, item: WorkItem) -> bool {
        if self.ctx.mailbox.enqueue(item).is_err() {
            return false;
        }
        self.ctx.wake.signal();
        true
    }
}

impl Drop for OsPriorityWorker {
    fn drop(&mut self) {
        self.ctx.halt.store(true, Ordering::Release);
        // Wake it so the halt is observed now rather than after the timeout.
        self.ctx.wake.signal();
        // Joining before the `Arc` is released is what makes the worker's reads
        // of `WorkerCtx` — and of the executor's arena — sound.
        if let Some(task) = self.task.take() {
            task.join();
        }
    }
}

/// The pool itself: at most one worker per distinct non-zero `os_pri`.
pub(crate) struct OsPriorityPool {
    workers: FnvIndexMap<u8, OsPriorityWorker, MAX_PRIORITY_LEVELS>,
    /// Priorities already tried and refused by the platform, so a failing
    /// `spawn` is attempted once rather than on every dispatch.
    unavailable: FnvIndexMap<u8, (), MAX_PRIORITY_LEVELS>,
}

impl OsPriorityPool {
    pub(crate) const fn new() -> Self {
        Self {
            workers: FnvIndexMap::new(),
            unavailable: FnvIndexMap::new(),
        }
    }

    /// Dispatch `item` at `os_pri`, spawning that level's worker on first use.
    /// `false` means the caller should dispatch cooperatively instead.
    pub(crate) fn try_dispatch(
        &mut self,
        os_pri: u8,
        apply_policy: fn(
            nros_platform_api::SchedPolicy,
        ) -> Result<(), nros_platform_api::SchedError>,
        item: WorkItem,
    ) -> bool {
        if self.unavailable.contains_key(&os_pri) {
            return false;
        }
        if !self.workers.contains_key(&os_pri) {
            let Some(worker) = OsPriorityWorker::spawn(os_pri, apply_policy) else {
                let _ = self.unavailable.insert(os_pri, ());
                return false;
            };
            if self.workers.insert(os_pri, worker).is_err() {
                // Pool full — this level runs cooperatively. Recorded so the
                // spawn is not retried (and immediately dropped) every cycle.
                let _ = self.unavailable.insert(os_pri, ());
                return false;
            }
        }
        self.workers
            .get(&os_pri)
            .is_some_and(|w| w.try_dispatch(item))
    }
}
