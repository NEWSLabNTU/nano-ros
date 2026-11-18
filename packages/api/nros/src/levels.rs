//! The Rust sched task runners' LEVEL
//! core: one executor per core, a worker per level over it.
//!
//! The C-ABI runners reach this through `nros-cpp`'s scheduler FFI
//! (`nros_cpp_level_open`, `nros_cpp_level_spin_once`); a Rust runner holds
//! the core's `ExecutorNodeRuntime` itself and uses the same three steps:
//!
//! 1. `LevelCore::new` -- the boot level's runtime, leaked for the image's
//!    life, with the port's recursive priority-inheriting mutex and the
//!    calling thread's identity installed as the executor's lock
//!    ([`nros_node::executor::exec_lock::ExecLock`]) when more than one level
//!    runs, the level count stated, and the boot level made current.
//! 2. `LevelCore::open_level` + `LevelCore::with_runtime` -- on a level's
//!    own thread: its worker record (leaked with the level), its waiter
//!    linked into the executor's arrival routing, then the image's setup run
//!    on the shared runtime with the level's groups active and the level
//!    registering, all under the lock.
//! 3. `LevelCore::spin_level_once` / `LevelCore::spin_boot_once` -- one
//!    pass of a level's worker (its band, the lock released around each
//!    callback, then the ticks of the components its level registered) or
//!    of the boot level (the executor's own worker: transport, executor-wide
//!    chores, the boot level's component ticks). A component
//!    ticks on the level that registered it, never on two.
//!
//! 4. `LevelCore::into_runtime`: once every other level's
//!    thread has stopped and been joined, the boot level takes the runtime
//!    back (the lock removed), so a hosted runner can close its session and
//!    return. An RTOS runner never calls it: its levels spin for the image's
//!    life.
//!
//! Every access goes through the raw runtime pointer with the lock taken
//! first, so no reference to the runtime or its executor is live across
//! another thread's pass.

use core::{ffi::c_void, time::Duration};

// phase-562 W10 -- re-exported for the `nros::main!` RTIC level runner, which
// builds each level's waiter as a `static`.
use nros_node::executor::{
    Band, BandWorker,
    exec_lock::{ExecGuard, ExecLock, ExecLockFn, ThreadIdFn},
};
pub use nros_node::executor::{BandWaitFn, BandWaiter, WorkerWakeFn};

use crate::{Executor, node_runtime::ExecutorNodeRuntime};

/// The core's runtime, shared by every level of the image. `Copy`: each
/// level's thread carries the handle.
#[derive(Clone, Copy)]
pub struct LevelCore {
    rt: *mut ExecutorNodeRuntime,
}

// SAFETY: every access takes the executor's lock first (module docs).
unsafe impl Send for LevelCore {}
unsafe impl Sync for LevelCore {}

/// Why a level could not be set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelError {
    /// More levels than `MAX_LEVELS`, or a level past the count.
    BadLevel,
    /// The port's mutex was NULL or would not initialise.
    NoLock,
}

impl LevelCore {
    /// Make `runtime` the core's level runtime (leaked for the image's life).
    ///
    /// `n_levels` is the image's level count and `boot_level` the level of
    /// the calling thread. With more than one level, `mutex` must be an
    /// initialised recursive priority-inheriting mutex that `lock` / `unlock`
    /// take and give and `thread_id` must name the calling thread: the
    /// executor carries them so every pass releases the lock around its
    /// callbacks and the levels preempt each other on one executor.
    ///
    /// # Safety
    /// Called before any second thread can reach the runtime. `mutex` lives
    /// for the image's life; `thread_id` is equal on one thread and distinct
    /// across live threads.
    pub unsafe fn new(
        runtime: ExecutorNodeRuntime,
        n_levels: u8,
        boot_level: u8,
        mutex: *mut c_void,
        lock: ExecLockFn,
        unlock: ExecLockFn,
        thread_id: ThreadIdFn,
    ) -> Result<Self, LevelError> {
        if usize::from(n_levels) > crate::MAX_LEVELS || boot_level >= n_levels.max(1) {
            return Err(LevelError::BadLevel);
        }
        let rt: &'static mut ExecutorNodeRuntime =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(runtime));
        if !crate::set_level_count(n_levels.max(1)) {
            return Err(LevelError::BadLevel);
        }
        let exec = rt.executor_mut();
        if n_levels > 1 {
            // SAFETY: the caller's contract on `mutex` and `thread_id`.
            let Some(l) = (unsafe { ExecLock::new(mutex, lock, unlock) }) else {
                return Err(LevelError::NoLock);
            };
            exec.set_exec_lock(Some(unsafe { l.with_thread_id(thread_id) }));
        }
        if !exec.set_level(boot_level) {
            return Err(LevelError::BadLevel);
        }
        exec.set_registering_level(boot_level);
        Ok(Self { rt })
    }

    /// The core's executor, through the raw pointer (no reference made).
    fn exec(&self) -> *mut Executor<'static> {
        // SAFETY: `rt` is the leaked runtime; only the field address is taken.
        unsafe { ExecutorNodeRuntime::executor_ptr(self.rt) }
    }

    /// Run `f` on the core's runtime with the lock held.
    ///
    /// # Safety
    /// No reference to the runtime obtained here outlives `f`.
    pub unsafe fn with_runtime<R>(&self, f: impl FnOnce(&mut ExecutorNodeRuntime) -> R) -> R {
        // SAFETY: the executor is live; the lock is taken before the reference.
        let _g = unsafe { ExecGuard::acquire(self.exec()) };
        // SAFETY: under the lock no other thread touches the runtime (the
        // callback windows of a pass touch only the executor's own state).
        f(unsafe { &mut *self.rt })
    }

    /// Open level `level` on the calling thread: its worker (leaked with the
    /// level) attached to the executor's arrival routing and the level made
    /// the registering one. Returns the worker the thread's passes use.
    ///
    /// # Safety
    /// Called once per level, on the level's own thread.
    pub unsafe fn open_level(&self, level: u8) -> Result<&'static mut BandWorker, LevelError> {
        unsafe { self.open_level_with(level, BandWaiter::platform()) }
    }

    // phase-562 W10.
    /// [`Self::open_level`] with the level's own waiter, for a
    /// runner whose levels cannot wait on the platform's wake object (an RTIC
    /// level runs to completion: its waiter's `wake` spawns its task, and its
    /// `wait` returns at once).
    ///
    /// # Safety
    /// As [`Self::open_level`].
    // The worker is LEAKED here (`Box::leak`), so the `&'static mut` is new
    // and unaliased; it borrows nothing from `waiter`, which the worker only
    // links to.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn open_level_with(
        &self,
        level: u8,
        waiter: Option<&'static BandWaiter>,
    ) -> Result<&'static mut BandWorker, LevelError> {
        if usize::from(level) >= crate::MAX_LEVELS {
            return Err(LevelError::BadLevel);
        }
        let worker = match waiter {
            Some(w) => BandWorker::new(level, Band::ALL).with_waiter(w),
            None => BandWorker::new(level, Band::ALL),
        };
        let worker: &'static mut BandWorker =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(worker));
        // SAFETY: as `with_runtime`.
        unsafe {
            self.with_runtime(|rt| {
                let exec = rt.executor_mut();
                exec.attach_band_worker(worker);
                exec.set_registering_level(level);
            })
        };
        Ok(worker)
    }

    /// One pass of a spawned level's worker: it parks (its waiter, the lock
    /// released) for at most `timeout` or until its band's next timer or an
    /// arrival, then runs its band's ready callbacks and the
    /// ticks of the components its level registered -- the executor's
    /// enrolled ones inside the band pass, the runtime's own after it, the
    /// lock held. Returns the pass's counts.
    ///
    /// # Safety
    /// `worker` is the calling thread's, from [`Self::open_level`].
    pub unsafe fn spin_level_once(
        &self,
        worker: &mut BandWorker,
        timeout: Duration,
    ) -> nros_node::SpinOnceResult {
        let exec = self.exec();
        // SAFETY: the lock is held once; no reference to the executor lives.
        let _g = unsafe { ExecGuard::acquire(exec) };
        let result = unsafe { Executor::spin_band_once(exec, worker, timeout) };
        // SAFETY: under the lock no other thread touches the runtime.
        unsafe { (*self.rt).run_ticks_for(Some(worker.level())) };
        result
    }

    /// The teardown: take the runtime back from the levels,
    /// the executor's lock removed, for the caller to close its session
    /// (`Executor::close`) and drop it. The level workers stay leaked, as
    /// they were for the image's life, and the level count stays stated.
    ///
    /// # Safety
    /// Called once, on the boot level's thread, after every other thread
    /// that reached the runtime (each level's worker, any callback it ran)
    /// has stopped and been joined; no copy of this `LevelCore` is used
    /// afterwards.
    pub unsafe fn into_runtime(self) -> ExecutorNodeRuntime {
        // SAFETY: `rt` came from `Box::leak` in `new`, and the caller's
        // contract makes this the only live handle to it.
        let mut rt = unsafe { alloc::boxed::Box::from_raw(self.rt) };
        rt.executor_mut().set_exec_lock(None);
        *rt
    }

    /// The budget a level runner passes to
    /// [`Self::spin_level_once`] / [`Self::spin_boot_once`]: the runtime's
    /// [`ExecutorNodeRuntime::entry_spin_budget`]. Each park already ends at
    /// the level's next deadline or on an arrival, so the budget is not a
    /// cadence; passing the tier's period instead woke every idle level once a
    /// period. Ask it after the levels' setup, which is what registers the
    /// components whose ticks keep the short budget.
    ///
    /// # Safety
    /// As [`Self::with_runtime`].
    pub unsafe fn entry_spin_budget(&self) -> Duration {
        // SAFETY: the caller's contract.
        unsafe { self.with_runtime(|rt| rt.entry_spin_budget()) }
    }

    /// Wake every level's worker and the boot level, for a
    /// runner that is stopping them: a level parked on the long budget would
    /// otherwise see its stop flag only at its next deadline. Lock-free.
    pub fn wake_all(&self) {
        // SAFETY: the runtime is live for the image's life (leaked in `new`);
        // `wake_all_workers` reads the executor's wake context, written at
        // open and never replaced, and signals lock-free.
        unsafe { (*self.exec()).wake_all_workers() }
    }

    /// One pass of the boot level: the executor's own worker (transport,
    /// executor-wide chores), then the boot level's components' ticks, the
    /// lock held except around each callback.
    ///
    /// # Safety
    /// Called from the boot level's thread only.
    pub unsafe fn spin_boot_once(&self, timeout: Duration) -> nros_node::SpinOnceResult {
        // SAFETY: as `spin_level_once`; the ticks run under the same guard.
        unsafe { ExecutorNodeRuntime::spin_once_shared(self.rt, timeout) }
    }
}

// phase-562 W10.
/// Levels that RUN TO COMPLETION: one pass per wake, never a
/// blocking wait.
///
/// An RTIC level is a software task at its own hardware priority. A level
/// that parked on a timed wait, as a threaded RTOS's level does, would hold
/// its priority and starve every level below it, the boot level's transport
/// included. So each non-boot level's task does ONE band pass
/// ([`LevelCore::spin_level_once`] with a zero timeout) per wake, and two
/// things wake it:
///
/// * an arrival in its band, through the level's [`BandWaiter`], whose `wake`
///   the image points at "spawn this level's task" ([`LevelCore::open_level_with`]);
/// * its band's next timer, through ONE board one-shot armed at the earliest
///   deadline over the non-boot levels ([`Self::rearm`]). Without it a higher
///   band's due timer would wait out the boot level's park, which is bounded
///   by the boot band's own timers only.
///
/// A band's deadline is read at the end of that level's own pass, the one
/// moment it is exact (a band's timers are credited their elapsed time by its
/// own passes), and kept as an absolute time per level.
///
/// A `const` static: the image's boot task [`Self::install`]s the core once,
/// before any level runs, and every level and the wake interrupt read it
/// after.
pub struct RunToCompletionLevels {
    core: core::cell::UnsafeCell<Option<LevelCore>>,
    installed: core::sync::atomic::AtomicBool,
    workers: [core::sync::atomic::AtomicPtr<BandWorker>; crate::MAX_LEVELS],
    /// Each level's next timer deadline.
    times: crate::level_wake::LevelWakeTimes,
}

// SAFETY: `core` is written once, by `install`, before `installed` is set,
// and only read after it is observed set; everything else is atomic.
unsafe impl Sync for RunToCompletionLevels {}

impl RunToCompletionLevels {
    /// An empty runner, for a `static`.
    pub const fn new() -> Self {
        Self {
            core: core::cell::UnsafeCell::new(None),
            installed: core::sync::atomic::AtomicBool::new(false),
            workers: [const { core::sync::atomic::AtomicPtr::new(core::ptr::null_mut()) };
                crate::MAX_LEVELS],
            times: crate::level_wake::LevelWakeTimes::new(),
        }
    }

    /// Hand the runner its core, once, before any level task runs. `false`
    /// when it already has one.
    ///
    /// # Safety
    /// No level task or wake interrupt reads this runner until it returns.
    pub unsafe fn install(&self, core: LevelCore, boot_level: u8) -> bool {
        use core::sync::atomic::Ordering;
        if self.installed.load(Ordering::Acquire) {
            return false;
        }
        unsafe { *self.core.get() = Some(core) };
        self.times.set_boot(boot_level);
        self.installed.store(true, Ordering::Release);
        true
    }

    /// The core, once installed.
    pub fn core(&self) -> Option<&LevelCore> {
        if self.installed.load(core::sync::atomic::Ordering::Acquire) {
            // SAFETY: written once before `installed` was set (`install`).
            unsafe { (*self.core.get()).as_ref() }
        } else {
            None
        }
    }

    /// Record `level`'s band worker (from [`LevelCore::open_level_with`]).
    pub fn set_worker(&self, level: u8, worker: &'static mut BandWorker) {
        if let Some(slot) = self.workers.get(usize::from(level)) {
            slot.store(worker, core::sync::atomic::Ordering::Release);
        }
    }

    /// One pass of `level`'s band, then its next deadline is recorded and the
    /// board one-shot re-armed through `arm`. A level with no worker (or a
    /// runner not yet installed) does nothing.
    ///
    /// # Safety
    /// Called only from `level`'s own task, never re-entered for one level.
    pub unsafe fn pass(&self, level: u8, arm: fn(Option<u64>)) {
        use core::sync::atomic::Ordering;
        let (Some(core), Some(slot)) = (self.core(), self.workers.get(usize::from(level))) else {
            return;
        };
        let worker = slot.load(Ordering::Acquire);
        if worker.is_null() {
            return;
        }
        // SAFETY: the worker is this level's alone, and its task is the only
        // caller for this level.
        let worker = unsafe { &mut *worker };
        let _ = unsafe { core.spin_level_once(worker, Duration::ZERO) };
        let next =
            unsafe { core.with_runtime(|rt| rt.executor().next_timer_deadline_of_level_us(level)) };
        self.times.set(level, next, crate::time::now_us() as u32);
        self.rearm(arm);
    }

    /// Arm the board one-shot at the earliest deadline over the non-boot
    /// levels, or disarm it when none has a timer.
    pub fn rearm(&self, arm: fn(Option<u64>)) {
        arm(self
            .times
            .soonest_after(crate::time::now_us() as u32)
            .map(u64::from));
    }

    /// Whether `level`'s next timer is due at `now` (the wake interrupt
    /// spawns exactly these).
    pub fn is_due(&self, level: u8, now: u64) -> bool {
        self.times.is_due(level, now as u32)
    }
}

impl Default for RunToCompletionLevels {
    fn default() -> Self {
        Self::new()
    }
}
