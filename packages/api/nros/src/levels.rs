//! phase-509 P10 (RFC-0108 "As built in P9b") -- the Rust tier runners' LEVEL
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
//!    callback) or of the boot level (the executor's own worker: transport,
//!    executor-wide chores, component ticks).
//!
//! Every access goes through the raw runtime pointer with the lock taken
//! first, so no reference to the runtime or its executor is live across
//! another thread's pass.

use core::{ffi::c_void, time::Duration};

use nros_node::executor::{
    Band, BandWaiter, BandWorker,
    exec_lock::{ExecGuard, ExecLock, ExecLockFn, ThreadIdFn},
};

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
        if usize::from(level) >= crate::MAX_LEVELS {
            return Err(LevelError::BadLevel);
        }
        let worker = match BandWaiter::platform() {
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
    /// arrival, then runs its band's ready callbacks.
    ///
    /// # Safety
    /// `worker` is the calling thread's, from [`Self::open_level`].
    pub unsafe fn spin_level_once(&self, worker: &mut BandWorker, timeout: Duration) {
        let exec = self.exec();
        // SAFETY: the lock is held once; no reference to the executor lives.
        let _g = unsafe { ExecGuard::acquire(exec) };
        let _ = unsafe { Executor::spin_band_once(exec, worker, timeout) };
    }

    /// One pass of the boot level: the executor's own worker (transport,
    /// executor-wide chores), then the components' ticks, the lock held
    /// except around each callback.
    ///
    /// # Safety
    /// Called from the boot level's thread only.
    pub unsafe fn spin_boot_once(&self, timeout: Duration) {
        // SAFETY: as `spin_level_once`; the ticks run under the same guard.
        unsafe { ExecutorNodeRuntime::spin_once_shared(self.rt, timeout) };
    }
}
