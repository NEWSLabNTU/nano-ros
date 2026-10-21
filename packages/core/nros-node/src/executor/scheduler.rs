// Copyright (c) 2026, NEWSLab NTU.
// SPDX-License-Identifier: Apache-2.0

//! phase-509 P6 (RFC-0108 D3, D7; amended RFC-0107 step 4) -- the scheduler /
//! worker split.
//!
//! RFC-0107's per-tier dispatcher was one value that both ORDERED ready work
//! and was the unit of PREEMPTION. RFC-0108 splits it in two:
//!
//! - a per-core [`Scheduler`]: the ordered ready set over the core's callback
//!   slots, each slot's dispatch level, and the band table that says which
//!   worker owns which levels;
//! - a per-level [`Worker`]: one contiguous [`Band`] of dispatch levels, its
//!   park (RFC-0106: the deadline sources, the primitive that blocks, and
//!   what the last park did) and its stack record (the headroom rule's
//!   bound and state). A worker is the thread that runs its band.
//!
//! **One level is today's executor.** [`Executor`](super::Executor) stays the
//! one-level facade: it holds one [`Worker`] whose band is every level
//! ([`Band::ALL`]), so its dispatch order, its park and its stack rule are the
//! ones it always had. The worker's fields are the park and stack fields the
//! executor used to hold inline, moved and not changed.
//!
//! **Preemption between bands is the kernel's** (RFC-0108 D6). Each worker
//! runs at its band's kernel priority and picks only its own band, most
//! urgent level first, registration order within a level. On one core a less
//! urgent worker runs only while every more urgent one is parked, so an
//! arrival in a more urgent band preempts a running callback of a less urgent
//! one, and the members of one band never preempt each other: an arrival in
//! the band of the running worker waits for its current callback to return.
//!
//! **Waking (D7).** An arrival the transport attributes to its slot
//! (RFC-0084) wakes only the worker of that slot's band
//! ([`Scheduler::arrive`]). An unattributed arrival can only say "something
//! on this session", so it wakes the most urgent parked worker, which
//! re-picks ([`Scheduler::arrive_unattributed`]).
//!
//! The scheduler's run-time half takes `&self` and is lock-free (one atomic
//! word of ready bits, one parked flag per worker), so the workers of one core
//! and the transport's arrival path may all reach it at once. Its set-up half
//! ([`Scheduler::add_worker`], [`Scheduler::bind_slot`]) takes `&mut self`
//! and runs before any worker starts.
//!
//! Crate-private until the level runners (phase-509 P9) open a scheduler and
//! its levels through the FFI; until then the executor uses only its one
//! [`Worker`], and the [`Scheduler`] is driven by its own tests.

// The multi-level half has no in-crate caller before P9.
#![cfg_attr(not(test), allow(dead_code))]

use core::ffi::c_void;

use portable_atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering::SeqCst};

use super::spin::{MAX_WAKE_SOURCES, ParkUntilFn, WakeSourceId, WakeSourceSlot};

const _: () = assert!(MAX_WAKE_SOURCES < 256, "Worker::wake_source_count is a u8");

/// The least urgent dispatch level ([`Priority::dispatch_level`]: `Critical`
/// 0, `Normal` 1 + rank, `BestEffort` 31).
///
/// [`Priority::dispatch_level`]: super::sched_context::Priority::dispatch_level
pub const MAX_DISPATCH_LEVEL: u8 = 31;

/// Callback slots one scheduler orders: one machine word of ready bits, as
/// the entity wake's ready word (`entity_wake::SLOTS`).
pub const SCHEDULER_SLOTS: usize = usize::BITS as usize;

/// Workers (preemption levels) one scheduler can host: the
/// `CONFIG_NROS_EXECUTOR_MAX_LEVELS` range's upper end.
pub const MAX_WORKERS: usize = 32;

/// A slot bound to no level yet.
const UNBOUND: u8 = u8::MAX;

/// A contiguous range of dispatch levels, inclusive at both ends; level 0 is
/// the most urgent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    /// The most urgent level in the band: its threshold (RFC-0108 D3).
    pub most_urgent: u8,
    /// The least urgent level in the band: its floor.
    pub least_urgent: u8,
}

impl Band {
    /// Every level: the one-level executor.
    pub const ALL: Band = Band {
        most_urgent: 0,
        least_urgent: MAX_DISPATCH_LEVEL,
    };

    /// The band `[most_urgent, least_urgent]`, or `None` when it is empty or
    /// reaches past [`MAX_DISPATCH_LEVEL`].
    pub const fn new(most_urgent: u8, least_urgent: u8) -> Option<Band> {
        if most_urgent > least_urgent || least_urgent > MAX_DISPATCH_LEVEL {
            None
        } else {
            Some(Band {
                most_urgent,
                least_urgent,
            })
        }
    }

    /// Whether `level` is in the band.
    pub const fn contains(&self, level: u8) -> bool {
        level >= self.most_urgent && level <= self.least_urgent
    }
}

/// One preemption level: its band, its park and its stack record.
///
/// The park (RFC-0106: the platform deadline sources, the one primitive that
/// blocks, and what the last park asked for and got) and the stack record
/// (the headroom rule's declared bound and state, phase-436 E3/E4) moved here
/// from [`Executor`](super::Executor) unchanged; the executor's park and
/// stack methods read and write its worker's. The fields stay flat, not in a
/// park and a stack sub-struct, so their small members share padding as they
/// did inline in the executor.
pub(crate) struct Worker {
    /// The dispatch levels this worker runs.
    pub(crate) band: Band,
    // ---- the park (RFC-0106) ----
    /// phase-436 -- the bound the last park was computed with, in us, and the
    /// source that won it. See `WakeSourceId`.
    pub(crate) last_park_bound_us: u64,
    pub(crate) last_park_source: WakeSourceId,
    /// phase-436 W2 -- the park bound after rounding UP to what the
    /// platform's wake primitive can express. Kept beside the request rather
    /// than replacing it: reporting both is what makes the rounding auditable
    /// instead of silent (issue 1194).
    pub(crate) last_park_achieved_us: u64,
    /// phase-436 W6 -- registered platform deadline sources, and how many are
    /// live. Inline table: no allocator on the targets this exists for.
    pub(crate) wake_sources: [Option<WakeSourceSlot>; MAX_WAKE_SOURCES],
    /// A `u8` (it was a `usize` inline in the executor) so it shares the
    /// worker's small fields' padding: `MAX_WAKE_SOURCES` is far below 256.
    pub(crate) wake_source_count: u8,
    /// The single thing that actually blocks. Many sources say WHEN; one
    /// primitive does the waiting.
    pub(crate) park_primitive: Option<(ParkUntilFn, *mut c_void)>,
    /// phase-436 W7.b (issue 1242) -- the finest park the INSTALLED primitive
    /// can actually express, in microseconds. `0` = none installed, fall back
    /// to the `wake_wait_ms` ABI floor.
    pub(crate) park_granularity_declared_us: u64,
    // ---- the stack record (phase-436 E3/E4) ----
    /// Declared minimum stack headroom in bytes for the thread this worker
    /// runs on; `0` (default) disables the rule. The entry that spawned the
    /// thread is the one that knows what it gave it.
    pub(crate) min_stack_headroom_bytes: usize,
    /// Lowest headroom already reported, so the rule fires on new lows only.
    /// `usize::MAX` = nothing reported yet.
    pub(crate) stack_headroom_reported: usize,
    /// phase-436 E3 -- a minimum is set but the port reports no stack data,
    /// so the rule will skip every tick.
    pub(crate) stack_headroom_blind: bool,
    /// phase-436 E4 -- when the headroom query last ran (`None` = never; a
    /// clockless build stores `Some(0)` to mean "has run"), spins since, and
    /// how many queries actually reached the port.
    pub(crate) stack_headroom_last_check_us: Option<u64>,
    pub(crate) stack_headroom_spins_since_check: u32,
    pub(crate) stack_headroom_checks: u32,
}

impl Worker {
    /// The one-level executor's worker: every dispatch level, nothing parked
    /// or recorded yet.
    pub(crate) const fn one_level() -> Self {
        Self {
            band: Band::ALL,
            last_park_bound_us: 0,
            last_park_source: WakeSourceId::CallerBudget,
            last_park_achieved_us: 0,
            wake_sources: [None; MAX_WAKE_SOURCES],
            wake_source_count: 0,
            park_primitive: None,
            park_granularity_declared_us: 0,
            min_stack_headroom_bytes: 0,
            stack_headroom_reported: usize::MAX,
            stack_headroom_blind: false,
            stack_headroom_last_check_us: None,
            stack_headroom_spins_since_check: 0,
            stack_headroom_checks: 0,
        }
    }
}

/// How a scheduler signals a worker's wake object (`NodeWake` on a real
/// port; a mock kernel in the tests).
pub type WorkerWakeFn = unsafe extern "C" fn(ctx: *mut c_void);

/// A worker's index in its scheduler, 0 the most urgent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkerId(pub u8);

/// Why a scheduler refused a set-up step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelSetupError {
    /// [`MAX_WORKERS`] workers are already added.
    TooManyWorkers,
    /// The band does not start right after the previous worker's (the first
    /// at level 0): bands partition the levels, most urgent first.
    NotContiguous { expected_most_urgent: u8, got: u8 },
    /// The slot is past [`SCHEDULER_SLOTS`], or the level past
    /// [`MAX_DISPATCH_LEVEL`].
    OutOfRange,
}

struct Seat {
    band: Band,
    wake: Option<(WorkerWakeFn, *mut c_void)>,
    parked: AtomicBool,
}

/// The per-core half: the ordered ready set and the band table.
pub struct Scheduler {
    /// Each slot's dispatch level; [`UNBOUND`] when no callback is bound.
    slot_level: [AtomicU8; SCHEDULER_SLOTS],
    /// One bit per slot with work queued.
    ready: AtomicUsize,
    seats: [Option<Seat>; MAX_WORKERS],
    workers: u8,
}

// SAFETY: the raw wake contexts are handed in by the port together with the
// function that reads them, and must be callable from any thread (the same
// contract as a backend's wake callback). Everything else is atomic or set up
// through `&mut self` before the workers start.
unsafe impl Sync for Scheduler {}
// SAFETY: as above; nothing in it is tied to the creating thread.
unsafe impl Send for Scheduler {}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            slot_level: [const { AtomicU8::new(UNBOUND) }; SCHEDULER_SLOTS],
            ready: AtomicUsize::new(0),
            seats: [const { None }; MAX_WORKERS],
            workers: 0,
        }
    }

    /// Add the next worker, less urgent than every one added before it. Its
    /// band must start right after the previous band's floor (the first at
    /// level 0): the bands partition the levels and are contiguous (RFC-0108
    /// D3). `wake` signals its wake object; `None` for a worker nothing has
    /// to wake (a test, or a worker that polls).
    pub fn add_worker(
        &mut self,
        band: Band,
        wake: Option<(WorkerWakeFn, *mut c_void)>,
    ) -> Result<WorkerId, LevelSetupError> {
        let idx = self.workers as usize;
        if idx >= MAX_WORKERS {
            return Err(LevelSetupError::TooManyWorkers);
        }
        let expected = match idx {
            0 => 0,
            _ => match &self.seats[idx - 1] {
                Some(prev) => prev.band.least_urgent + 1,
                None => 0,
            },
        };
        if band.most_urgent != expected {
            return Err(LevelSetupError::NotContiguous {
                expected_most_urgent: expected,
                got: band.most_urgent,
            });
        }
        self.seats[idx] = Some(Seat {
            band,
            wake,
            parked: AtomicBool::new(false),
        });
        self.workers += 1;
        Ok(WorkerId(idx as u8))
    }

    /// Workers added.
    pub fn worker_count(&self) -> usize {
        self.workers as usize
    }

    /// Bind callback `slot` to dispatch `level`.
    pub fn bind_slot(&mut self, slot: usize, level: u8) -> Result<(), LevelSetupError> {
        if slot >= SCHEDULER_SLOTS || level > MAX_DISPATCH_LEVEL {
            return Err(LevelSetupError::OutOfRange);
        }
        self.slot_level[slot].store(level, SeqCst);
        Ok(())
    }

    /// The worker whose band holds `level`; `None` when no band does.
    pub fn worker_of(&self, level: u8) -> Option<WorkerId> {
        self.seats[..self.workers as usize]
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.band.contains(level)))
            .map(|i| WorkerId(i as u8))
    }

    /// The band `worker` runs.
    pub fn band(&self, worker: WorkerId) -> Option<Band> {
        self.seat(worker).map(|s| s.band)
    }

    fn seat(&self, worker: WorkerId) -> Option<&Seat> {
        self.seats
            .get(worker.0 as usize)
            .and_then(Option::as_ref)
            .filter(|_| (worker.0) < self.workers)
    }

    fn wake(&self, worker: WorkerId) {
        if let Some(seat) = self.seat(worker)
            && let Some((wake, ctx)) = seat.wake
        {
            // SAFETY: the port handed `wake` and `ctx` in together, callable
            // from any thread (see `add_worker`).
            unsafe { wake(ctx) };
        }
    }

    /// RFC-0108 D7: an arrival attributed to `slot` queues it and wakes only
    /// the worker of its band. Returns that worker; `None` when the slot is
    /// bound to no level a band holds, which is then woken as an
    /// unattributed arrival would be.
    pub fn arrive(&self, slot: usize) -> Option<WorkerId> {
        let Some(level) = self.slot_level.get(slot).map(|l| l.load(SeqCst)) else {
            return self.arrive_unattributed();
        };
        // The bit first, so a worker the wake reaches sees it.
        self.ready.fetch_or(1usize << slot, SeqCst);
        match self.worker_of(level) {
            Some(w) => {
                self.wake(w);
                Some(w)
            }
            None => self.arrive_unattributed(),
        }
    }

    /// RFC-0108 D7: an arrival nothing attributes wakes the most urgent
    /// parked worker, which re-picks. `None` when no worker is parked: a
    /// running worker re-picks after its current callback anyway.
    pub fn arrive_unattributed(&self) -> Option<WorkerId> {
        let w = self.seats[..self.workers as usize]
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.parked.load(SeqCst)))
            .map(|i| WorkerId(i as u8))?;
        self.wake(w);
        Some(w)
    }

    /// Record that `worker` is parked (`true`) or running (`false`).
    pub fn set_parked(&self, worker: WorkerId, parked: bool) {
        if let Some(seat) = self.seat(worker) {
            seat.parked.store(parked, SeqCst);
        }
    }

    /// Whether `worker` is parked.
    pub fn is_parked(&self, worker: WorkerId) -> bool {
        self.seat(worker).is_some_and(|s| s.parked.load(SeqCst))
    }

    /// Whether any slot in `worker`'s band has work queued.
    pub fn has_work(&self, worker: WorkerId) -> bool {
        let Some(band) = self.band(worker) else {
            return false;
        };
        let ready = self.ready.load(SeqCst);
        (0..SCHEDULER_SLOTS)
            .any(|s| ready & (1usize << s) != 0 && band.contains(self.slot_level[s].load(SeqCst)))
    }

    /// Take the most urgent queued slot of `worker`'s band: the lowest
    /// dispatch level, registration (slot) order within a level. `None`
    /// when its band has nothing queued.
    pub fn pick(&self, worker: WorkerId) -> Option<usize> {
        let band = self.band(worker)?;
        loop {
            let ready = self.ready.load(SeqCst);
            let mut best: Option<(u8, usize)> = None;
            let mut bits = ready;
            while bits != 0 {
                let s = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let level = self.slot_level[s].load(SeqCst);
                if band.contains(level) && best.is_none_or(|(l, _)| level < l) {
                    best = Some((level, s));
                }
            }
            let (_, slot) = best?;
            let bit = 1usize << slot;
            if self
                .ready
                .compare_exchange(ready, ready & !bit, SeqCst, SeqCst)
                .is_ok()
            {
                return Some(slot);
            }
        }
    }

    /// Run `worker`'s band until it has nothing queued: pick, dispatch, pick
    /// again from the ready set as it is now (RFC-0108 D2). Returns how many
    /// callbacks ran.
    pub fn run(&self, worker: WorkerId, mut dispatch: impl FnMut(usize)) -> usize {
        let mut ran = 0;
        while let Some(slot) = self.pick(worker) {
            dispatch(slot);
            ran += 1;
        }
        ran
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{boxed::Box, cell::RefCell, format, string::String, vec, vec::Vec};

    /// A worker index carried as a wake context.
    fn ctx(i: usize) -> *mut c_void {
        core::ptr::without_provenance_mut(i)
    }

    #[test]
    fn bands_partition_the_levels_most_urgent_first() {
        let mut s = Scheduler::new();
        assert_eq!(
            s.add_worker(Band::new(1, 3).unwrap(), None),
            Err(LevelSetupError::NotContiguous {
                expected_most_urgent: 0,
                got: 1
            })
        );
        assert_eq!(
            s.add_worker(Band::new(0, 3).unwrap(), None),
            Ok(WorkerId(0))
        );
        assert!(matches!(
            s.add_worker(Band::new(5, 9).unwrap(), None),
            Err(LevelSetupError::NotContiguous { .. })
        ));
        assert_eq!(
            s.add_worker(Band::new(4, 31).unwrap(), None),
            Ok(WorkerId(1))
        );
        assert_eq!(s.worker_count(), 2);
        assert_eq!(s.worker_of(2), Some(WorkerId(0)));
        assert_eq!(s.worker_of(4), Some(WorkerId(1)));
        assert_eq!(Band::new(3, 2), None);
        assert_eq!(Band::new(0, 32), None);
    }

    /// One band is today's order: most urgent level first, registration
    /// order within a level.
    #[test]
    fn one_band_picks_by_level_then_registration() {
        let mut s = Scheduler::new();
        s.add_worker(Band::ALL, None).unwrap();
        for (slot, level) in [(0, 3), (1, 1), (2, 3), (3, 1)] {
            s.bind_slot(slot, level).unwrap();
        }
        for slot in [2, 0, 3, 1] {
            s.arrive(slot);
        }
        let mut order = vec![];
        s.run(WorkerId(0), |slot| order.push(slot));
        assert_eq!(order, [1, 3, 0, 2]);
    }

    /// D7: an attributed arrival wakes only its band's worker; an
    /// unattributed one the most urgent parked worker.
    #[test]
    fn an_arrival_wakes_only_its_bands_worker() {
        static WOKEN: [AtomicU8; 2] = [AtomicU8::new(0), AtomicU8::new(0)];
        unsafe extern "C" fn wake(ctx: *mut c_void) {
            WOKEN[ctx as usize].fetch_add(1, SeqCst);
        }
        let mut s = Scheduler::new();
        s.add_worker(Band::new(0, 1).unwrap(), Some((wake, ctx(0))))
            .unwrap();
        s.add_worker(Band::new(2, 31).unwrap(), Some((wake, ctx(1))))
            .unwrap();
        s.bind_slot(0, 1).unwrap();
        s.bind_slot(1, 5).unwrap();
        assert_eq!(s.arrive(1), Some(WorkerId(1)));
        assert_eq!(
            (WOKEN[0].load(SeqCst), WOKEN[1].load(SeqCst)),
            (0, 1),
            "a band-1 arrival does not wake band 0"
        );
        assert_eq!(s.arrive(0), Some(WorkerId(0)));
        assert_eq!((WOKEN[0].load(SeqCst), WOKEN[1].load(SeqCst)), (1, 1));
        // Nobody parked: an unattributed arrival wakes nobody (each running
        // worker re-picks after its callback).
        assert_eq!(s.arrive_unattributed(), None);
        s.set_parked(WorkerId(1), true);
        assert!(s.is_parked(WorkerId(1)) && !s.is_parked(WorkerId(0)));
        assert_eq!(s.arrive_unattributed(), Some(WorkerId(1)));
        s.set_parked(WorkerId(0), true);
        assert_eq!(s.arrive_unattributed(), Some(WorkerId(0)));
        // Each worker picks only its own band.
        assert_eq!(s.pick(WorkerId(0)), Some(0));
        assert_eq!(s.pick(WorkerId(0)), None);
        assert!(s.has_work(WorkerId(1)));
        assert_eq!(s.pick(WorkerId(1)), Some(1));
    }

    // ---- a deterministic one-core kernel -------------------------------
    //
    // Fixed-priority preemptive, one core, no time: a woken worker more
    // urgent than the one running runs at once (nested inside the running
    // callback, which is where a real kernel would preempt it); any other
    // wake is pending until the running worker returns. Each callback is a
    // script of arrivals it raises mid-run.

    struct Kernel {
        sched: &'static Scheduler,
        running: Vec<WorkerId>,
        pending: Vec<WorkerId>,
        scripts: Vec<Vec<usize>>,
        trace: Vec<String>,
    }

    std::thread_local! {
        static KERNEL: RefCell<Option<Kernel>> = const { RefCell::new(None) };
    }

    fn with<R>(f: impl FnOnce(&mut Kernel) -> R) -> R {
        KERNEL.with(|k| f(k.borrow_mut().as_mut().expect("kernel installed")))
    }

    unsafe extern "C" fn kernel_wake(ctx: *mut c_void) {
        let w = WorkerId(ctx as usize as u8);
        let preempts = with(|k| {
            let current = k.running.last().copied();
            if current.is_none_or(|c| w < c) {
                true
            } else {
                if !k.pending.contains(&w) && current != Some(w) {
                    k.pending.push(w);
                }
                false
            }
        });
        if preempts {
            run_worker(w);
        }
    }

    fn run_worker(w: WorkerId) {
        let sched = with(|k| {
            k.running.push(w);
            k.sched
        });
        sched.set_parked(w, false);
        sched.run(w, |slot| {
            let script = with(|k| {
                k.trace.push(format!("start {slot}"));
                k.scripts[slot].clone()
            });
            for raised in script {
                sched.arrive(raised);
            }
            with(|k| k.trace.push(format!("end {slot}")));
        });
        sched.set_parked(w, true);
        with(|k| k.running.pop());
        // The kernel's next pick: the most urgent pending worker.
        loop {
            let next = with(|k| {
                k.pending.sort();
                if k.pending.is_empty() {
                    None
                } else {
                    Some(k.pending.remove(0))
                }
            });
            match next {
                Some(n) => run_worker(n),
                None => break,
            }
        }
    }

    /// Band 0 (levels 0-1) holds `H`; band 1 (levels 2-31) holds `A` and
    /// `B`, `B` the more urgent of the two. Slots: H 0, A 1, B 2.
    fn two_bands(scripts: Vec<Vec<usize>>) -> &'static Scheduler {
        let mut s = Scheduler::new();
        s.add_worker(Band::new(0, 1).unwrap(), Some((kernel_wake, ctx(0))))
            .unwrap();
        s.add_worker(Band::new(2, 31).unwrap(), Some((kernel_wake, ctx(1))))
            .unwrap();
        s.bind_slot(0, 1).unwrap(); // H
        s.bind_slot(1, 6).unwrap(); // A
        s.bind_slot(2, 4).unwrap(); // B
        let s: &'static Scheduler = Box::leak(Box::new(s));
        s.set_parked(WorkerId(0), true);
        s.set_parked(WorkerId(1), true);
        KERNEL.with(|k| {
            *k.borrow_mut() = Some(Kernel {
                sched: s,
                running: vec![],
                pending: vec![],
                scripts,
                trace: vec![],
            })
        });
        s
    }

    fn trace() -> Vec<String> {
        with(|k| k.trace.clone())
    }

    /// RFC-0108 P6 acceptance: an arrival in the more urgent band preempts a
    /// running callback of the less urgent band.
    #[test]
    fn a_more_urgent_bands_arrival_preempts_a_running_callback() {
        // A raises H mid-run.
        let s = two_bands(vec![vec![], vec![0], vec![]]);
        s.arrive(1);
        assert_eq!(trace(), ["start 1", "start 0", "end 0", "end 1"]);
    }

    /// RFC-0108 P6 acceptance: members of one band never preempt each other.
    /// `B` is more urgent than `A` but in its band, so it waits for `A`.
    #[test]
    fn members_of_one_band_never_preempt_each_other() {
        // A raises B mid-run.
        let s = two_bands(vec![vec![], vec![2], vec![]]);
        s.arrive(1);
        assert_eq!(trace(), ["start 1", "end 1", "start 2", "end 2"]);
    }

    /// A less urgent band's arrival raised by the more urgent band waits
    /// for it, and the band's own order holds when both of its members are
    /// queued: `B` before `A`.
    #[test]
    fn a_less_urgent_arrival_waits_and_the_band_keeps_its_order() {
        // H raises A and B mid-run.
        let s = two_bands(vec![vec![1, 2], vec![], vec![]]);
        s.arrive(0);
        assert_eq!(
            trace(),
            ["start 0", "end 0", "start 2", "end 2", "start 1", "end 1"]
        );
    }
}
