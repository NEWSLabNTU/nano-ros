//! phase-509 P9b item 1 (RFC-0108 "As built in P9b") -- the executor's lock.
//!
//! RFC-0108 runs one executor per core with one worker thread per preemption
//! level. Every path that touches the executor's state -- a worker's pass, and
//! every C / C++ FFI entry a callback or another thread calls -- then has to be
//! serialised, and that is this lock: a port-provided, RECURSIVE,
//! PRIORITY-INHERITING mutex (`nros_platform_mutex_rec_*` on every port), which
//! the runner installs on the executor before a second thread can reach it.
//!
//! An executor with no lock installed (every one-level image, every Rust
//! `&mut` user) takes the `None` arm: a guard is one predictable branch and
//! nothing else, so this seam changes no behaviour where it is not installed.
//!
//! Why recursive: an owned pass (`spin_once(&mut self)`), and a pass whose
//! lock has no thread identity, hold the lock across each user callback, so a
//! callback that calls back into the executor (`timer_cancel`, `*_release`, a
//! parameter call) re-enters it on the same thread.
//!
//! Item 2 -- the callback window. A shared or band pass
//! (`Executor::spin_once_shared`, `Executor::spin_band_once`) whose lock
//! carries the port's thread identity (`ExecLock::with_thread_id`) gives
//! the lock for each user callback and takes it back after (`Window`). The
//! entry stays marked running (its slot tag), so no other pass dispatches or
//! probes it and its release is deferred; a `Running` record names the
//! thread running it, so an entry operation from another thread is told apart
//! from one the callback makes itself: the entry accessors answer as for a
//! stale handle, and a timer cancel or reset is deferred to the callback's
//! return.
//!
//! Why priority-inheriting: a less urgent worker holding the lock for a
//! bookkeeping step must run at the waiter's priority, so a more urgent
//! level's blocking is bounded by ONE critical section (RFC-0108 B_i).
//!
//! Interrupt handlers never take it: arrivals go through the scheduler's
//! lock-free ready word and the wake object, exactly as before.

use core::{ffi::c_void, ptr::NonNull};

/// A port's lock or unlock entry, `nros_platform_mutex_rec_lock`'s shape:
/// `0` on success, negative on failure.
pub type ExecLockFn = unsafe extern "C" fn(*mut c_void) -> i8;

/// The calling thread's identity, as the port names it (`k_current_get`,
/// `pthread_self`): equal on one thread, distinct across live threads.
pub type ThreadIdFn = unsafe extern "C" fn() -> usize;

/// The executor's lock: the port's recursive priority-inheriting mutex and
/// the two calls that take and give it. Installed once, before any second
/// thread can reach the executor ([`super::Executor::set_exec_lock`]), and
/// never changed while one can.
#[derive(Clone, Copy, Debug)]
pub struct ExecLock {
    ctx: NonNull<c_void>,
    lock: ExecLockFn,
    unlock: ExecLockFn,
    /// phase-509 P9b item 2 -- the calling thread's identity. With it, a pass
    /// releases the lock around each user callback (a "window"), and an
    /// operation on an entry whose callback runs in another thread's window
    /// is told apart from one the callback makes itself. Without it the lock
    /// is held for the whole pass, as in item 1.
    #[cfg_attr(not(any(has_rmw, test)), allow(dead_code))] // read by the pass
    thread_id: Option<ThreadIdFn>,
}

// SAFETY: the context is the port's mutex, whose whole purpose is to be shared
// between threads; the two entries are plain functions.
unsafe impl Send for ExecLock {}
// SAFETY: as above -- `lock` / `unlock` are the port's thread-safe calls.
unsafe impl Sync for ExecLock {}

impl ExecLock {
    /// A lock over the port's mutex at `ctx`, already initialised by the
    /// caller. `None` for a NULL `ctx`.
    ///
    /// # Safety
    /// `ctx` is NULL or an initialised recursive mutex that `lock` / `unlock`
    /// take and give, and it outlives every executor the lock is installed on.
    pub unsafe fn new(ctx: *mut c_void, lock: ExecLockFn, unlock: ExecLockFn) -> Option<Self> {
        Some(Self {
            ctx: NonNull::new(ctx)?,
            lock,
            unlock,
            thread_id: None,
        })
    }

    /// This lock, with the port's thread identity: a pass that holds it
    /// releases it around each user callback (phase-509 P9b item 2).
    ///
    /// # Safety
    /// `id` answers the calling thread's identity, from any thread, equal on
    /// one thread and distinct across threads that are alive together.
    pub unsafe fn with_thread_id(self, id: ThreadIdFn) -> Self {
        Self {
            thread_id: Some(id),
            ..self
        }
    }

    /// Whether a pass may release this lock around a callback: only when it
    /// can tell the callback's own thread from the others.
    #[cfg(any(has_rmw, test))]
    pub(crate) fn opens_windows(&self) -> bool {
        self.thread_id.is_some()
    }

    /// The calling thread's identity; `None` without a [`ThreadIdFn`].
    #[cfg(any(has_rmw, test))]
    pub(crate) fn current_thread(&self) -> Option<usize> {
        // SAFETY: `with_thread_id`'s contract: callable from any thread.
        self.thread_id.map(|id| unsafe { id() })
    }

    pub(crate) fn take(&self) {
        // SAFETY: `new`'s contract: the context is the initialised mutex these
        // entries take. A failed take is not recoverable here (the port's
        // mutex never fails on a valid handle); the guard proceeds, which is
        // the behaviour the image had before the lock existed.
        let _ = unsafe { (self.lock)(self.ctx.as_ptr()) };
    }

    pub(crate) fn give(&self) {
        // SAFETY: as `take`; only ever called by the guard that took it.
        let _ = unsafe { (self.unlock)(self.ctx.as_ptr()) };
    }
}

/// Holds the executor's lock for its own lifetime: taken at
/// [`ExecGuard::acquire`], given at drop. With no lock installed it holds
/// nothing.
#[must_use = "the lock is given back when the guard drops"]
pub struct ExecGuard {
    lock: Option<ExecLock>,
}

impl ExecGuard {
    /// Take `lock`, if there is one.
    pub fn take(lock: Option<ExecLock>) -> Self {
        if let Some(l) = &lock {
            l.take();
        }
        Self { lock }
    }

    /// Take the lock installed on the executor at `exec`, BEFORE any reference
    /// to that executor exists -- the order that keeps a second thread's
    /// reference from overlapping this one.
    ///
    /// # Safety
    /// `exec` points at a live, initialised executor. Its lock field is read
    /// without a reference: it is written only while no second thread can
    /// reach the executor ([`super::Executor::set_exec_lock`]).
    #[cfg(any(has_rmw, test))]
    pub unsafe fn acquire(exec: *const super::Executor<'_>) -> Self {
        // SAFETY: the caller's contract; a field read through the raw pointer
        // creates no reference to the executor.
        let lock = unsafe { core::ptr::read(core::ptr::addr_of!((*exec).exec_lock)) };
        Self::take(lock)
    }

    /// Whether this guard holds a lock.
    pub fn holds(&self) -> bool {
        self.lock.is_some()
    }
}

impl Drop for ExecGuard {
    fn drop(&mut self) {
        if let Some(l) = &self.lock {
            l.give();
        }
    }
}

/// phase-509 P9b item 2 -- a callback window: the lock is given for this
/// value's lifetime and taken back when it drops, also on unwind, so the
/// guard the pass's caller holds always gives a lock it owns.
#[cfg(any(has_rmw, test))]
#[must_use = "the lock is taken back when the window drops"]
pub(crate) struct Window(ExecLock);

#[cfg(any(has_rmw, test))]
impl Window {
    /// Give `lock`, which the calling thread holds exactly once.
    pub(crate) fn open(lock: ExecLock) -> Self {
        lock.give();
        Self(lock)
    }
}

#[cfg(any(has_rmw, test))]
impl Drop for Window {
    fn drop(&mut self) {
        self.0.take();
    }
}

/// phase-509 P9b item 2 -- which thread runs the callback of `slot` in an
/// open window. Lives in the pass's frame for the window's length and is
/// linked into the executor's list under the lock (`Executor::running`).
#[cfg(any(has_rmw, test))]
pub(crate) struct Running {
    pub(crate) slot: usize,
    pub(crate) thread: usize,
    pub(crate) next: *mut Running,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicI32, Ordering};

    /// A counting "mutex": the depth it is held at, and how often it was taken.
    #[derive(Default)]
    struct Counter {
        depth: AtomicI32,
        takes: AtomicI32,
    }

    unsafe extern "C" fn lock(c: *mut c_void) -> i8 {
        // SAFETY: the test passes a `Counter` it owns as the context.
        let c = unsafe { &*(c as *const Counter) };
        c.depth.fetch_add(1, Ordering::SeqCst);
        c.takes.fetch_add(1, Ordering::SeqCst);
        0
    }

    unsafe extern "C" fn unlock(c: *mut c_void) -> i8 {
        // SAFETY: as `lock`.
        let c = unsafe { &*(c as *const Counter) };
        c.depth.fetch_sub(1, Ordering::SeqCst);
        0
    }

    /// A guard takes once and gives once, nests (the recursive case a
    /// callback re-entering the executor is), and with no lock does nothing.
    #[test]
    fn a_guard_takes_once_and_gives_on_drop_and_nests() {
        let c = Counter::default();
        // SAFETY: `c` outlives every guard taken below.
        let l = unsafe { ExecLock::new(&c as *const Counter as *mut c_void, lock, unlock) };
        {
            let g = ExecGuard::take(l);
            assert!(g.holds());
            assert_eq!(c.depth.load(Ordering::SeqCst), 1);
            {
                let _inner = ExecGuard::take(l);
                assert_eq!(c.depth.load(Ordering::SeqCst), 2);
            }
            assert_eq!(c.depth.load(Ordering::SeqCst), 1);
        }
        assert_eq!(c.depth.load(Ordering::SeqCst), 0);
        assert_eq!(c.takes.load(Ordering::SeqCst), 2);
        let none = ExecGuard::take(None);
        assert!(!none.holds());
        // SAFETY: a NULL context is refused before anything is called.
        assert!(unsafe { ExecLock::new(core::ptr::null_mut(), lock, unlock) }.is_none());
    }

    /// A spin lock standing in for the port mutex.
    struct Spin(core::sync::atomic::AtomicBool);

    unsafe extern "C" fn spin_lock(c: *mut c_void) -> i8 {
        // SAFETY: the test passes the static `Spin` as the context.
        let s = unsafe { &*(c as *const Spin) };
        while s
            .0
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        0
    }

    unsafe extern "C" fn spin_unlock(c: *mut c_void) -> i8 {
        // SAFETY: as `spin_lock`.
        let s = unsafe { &*(c as *const Spin) };
        s.0.store(false, Ordering::Release);
        0
    }

    /// Two threads that touch shared state only under the guard never
    /// interleave: every unsynchronised increment survives.
    #[test]
    fn guarded_sections_on_two_threads_do_not_interleave() {
        struct Shared(core::cell::UnsafeCell<u64>);
        // SAFETY: only touched under the guard below.
        unsafe impl Sync for Shared {}
        static LOCK: Spin = Spin(core::sync::atomic::AtomicBool::new(false));
        static SHARED: Shared = Shared(core::cell::UnsafeCell::new(0));
        let l =
            // SAFETY: `LOCK` is a static, so it outlives both threads.
            unsafe { ExecLock::new(&LOCK as *const Spin as *mut c_void, spin_lock, spin_unlock) };
        const N: u64 = 100_000;
        let worker = move || {
            for _ in 0..N {
                let _g = ExecGuard::take(l);
                // SAFETY: the guard serialises every access to SHARED.
                unsafe {
                    let p = SHARED.0.get();
                    let v = core::ptr::read_volatile(p);
                    core::ptr::write_volatile(p, v + 1);
                }
            }
        };
        let a = std::thread::spawn(worker);
        let b = std::thread::spawn(worker);
        a.join().unwrap();
        b.join().unwrap();
        let _g = ExecGuard::take(l);
        // SAFETY: both threads have joined and the guard is held.
        assert_eq!(unsafe { *SHARED.0.get() }, 2 * N);
    }
}
