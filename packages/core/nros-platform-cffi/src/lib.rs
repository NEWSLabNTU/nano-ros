//! C vtable adapter for the nros platform abstraction.
//!
//! This crate provides a vtable-based bridge so that platform implementations
//! written in C (or any language with a C ABI) can satisfy the nros platform
//! traits without writing Rust code.
//!
//! # Usage (C platform implementor)
//!
//! 1. Include `<nros/platform_vtable.h>`
//! 2. Fill in all function pointers in `nros_platform_vtable_t`
//! 3. Call `nros_platform_cffi_register(&my_vtable)` before opening a session
//!
//! # Usage (Rust consumer)
//!
//! Enable the `platform-cffi` feature on `nros-platform`. The
//! [`CffiPlatform`] zero-sized type implements all platform traits by
//! dispatching through the registered vtable.

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::{ffi::c_void, sync::atomic::Ordering};

use portable_atomic::AtomicPtr;

// ============================================================================
// Vtable definition (mirrors C header)
// ============================================================================

/// C function table for a platform implementation.
///
/// All function pointers are required. For capabilities the platform does
/// not support (e.g., threading on bare-metal), provide stubs that return 0
/// (success) for mutex/condvar ops and -1 for `task_init`.
///
/// # Return-value conventions
///
/// - `i8` returns: `0` = success, non-zero = error.
/// - Pointer returns: `NULL` indicates allocation failure or
///   not-implemented; non-`NULL` is the resource handle.
/// - `clock_*` and `time_*` returns are absolute / monotonic counters and
///   should never error — if the platform has no clock, return `0`.
///
/// # Threading
///
/// The vtable itself is registered once via
/// [`nros_platform_cffi_register`] and is read concurrently by every
/// nros entity. Function pointers must be safe to invoke from any
/// thread. `mutex_*` / `condvar_*` operations must be safe under
/// concurrent callers; `mutex_rec_*` must support recursive locking
/// from the same thread (zenoh-pico re-enters the same mutex).
#[repr(C)]
pub struct NrosPlatformVtable {
    // -- Clock --
    /// Monotonic milliseconds since some platform-defined epoch (boot,
    /// program start, …). Never decreases. Wraps after ~584 million years.
    pub clock_ms: unsafe extern "C" fn() -> u64,
    /// Monotonic microseconds since the same epoch as `clock_ms`. Used
    /// for fine-grained spin / wait deadlines.
    pub clock_us: unsafe extern "C" fn() -> u64,

    // -- Alloc --
    /// Allocate `size` bytes; return `NULL` on failure. May be called
    /// from any thread once the platform is registered.
    pub alloc: unsafe extern "C" fn(size: usize) -> *mut c_void,
    /// Resize the block at `ptr` to `size` bytes. Equivalent to libc
    /// `realloc`: `NULL` ptr → fresh alloc; `0` size → free + return
    /// `NULL`. Must preserve the contents up to `min(old, new)`.
    pub realloc: unsafe extern "C" fn(ptr: *mut c_void, size: usize) -> *mut c_void,
    /// Free a previously allocated block. `NULL` is a no-op.
    pub dealloc: unsafe extern "C" fn(ptr: *mut c_void),

    // -- Sleep --
    /// Sleep at least `us` microseconds. Spin if the platform clock has
    /// no sub-millisecond timer.
    pub sleep_us: unsafe extern "C" fn(us: usize),
    /// Sleep at least `ms` milliseconds.
    pub sleep_ms: unsafe extern "C" fn(ms: usize),
    /// Sleep at least `s` seconds.
    pub sleep_s: unsafe extern "C" fn(s: usize),

    // -- Yield (Phase 77.22) --
    /// Voluntarily yield the current task / thread. On bare-metal,
    /// `core::hint::spin_loop()` is acceptable; on RTOSes use the
    /// native cooperative-yield primitive (`k_yield`, `vPortYield`,
    /// `tx_thread_relinquish`, `sched_yield`, …). **Must be ISR-safe**
    /// only on bare-metal — RTOS yields are explicitly *not* safe from
    /// an ISR.
    pub yield_now: unsafe extern "C" fn(),

    // -- Random --
    /// Random `u8`. Should be cryptographically random where the
    /// platform has an entropy source; otherwise a seeded PRNG is
    /// acceptable. **Must be deterministic** within a single test
    /// session for reproducibility.
    pub random_u8: unsafe extern "C" fn() -> u8,
    /// Random `u16`. See `random_u8` notes.
    pub random_u16: unsafe extern "C" fn() -> u16,
    /// Random `u32`. See `random_u8` notes.
    pub random_u32: unsafe extern "C" fn() -> u32,
    /// Random `u64`. See `random_u8` notes.
    pub random_u64: unsafe extern "C" fn() -> u64,
    /// Fill `len` bytes at `buf` with random data.
    pub random_fill: unsafe extern "C" fn(buf: *mut c_void, len: usize),

    // -- Time (wall clock) --
    /// Wall-clock milliseconds since the Unix epoch, or `0` if the
    /// platform has no real-time clock.
    pub time_now_ms: unsafe extern "C" fn() -> u64,
    /// Whole seconds since the Unix epoch (truncated `time_now_ms`).
    pub time_since_epoch_secs: unsafe extern "C" fn() -> u32,
    /// Sub-second nanosecond component of the wall clock (`0..1e9`).
    pub time_since_epoch_nanos: unsafe extern "C" fn() -> u32,

    // -- Threading --
    /// Spawn a new task. `task` is opaque caller-provided storage
    /// (size determined by the implementor); `attr` carries scheduling
    /// hints (priority, stack size, …) or is `NULL` for defaults;
    /// `entry` is the task entry point; `arg` is forwarded to `entry`.
    /// Returns `0` on success, non-zero on failure.
    pub task_init: unsafe extern "C" fn(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8,
    /// Block until `task` exits. Cleans up the task storage on success.
    pub task_join: unsafe extern "C" fn(task: *mut c_void) -> i8,
    /// Mark `task` as detached — its storage is reclaimed on exit
    /// without a join.
    pub task_detach: unsafe extern "C" fn(task: *mut c_void) -> i8,
    /// Request `task` to terminate at the next cancellation point.
    /// Cooperative: a task that never reaches a cancel point will not
    /// stop.
    pub task_cancel: unsafe extern "C" fn(task: *mut c_void) -> i8,
    /// Terminate the calling task immediately. Does not return.
    pub task_exit: unsafe extern "C" fn(),
    /// Free the task storage allocated by `task_init`. Called after
    /// `task_join` or `task_detach + exit`.
    pub task_free: unsafe extern "C" fn(task: *mut *mut c_void),

    /// Initialise a non-recursive mutex in the caller-provided storage.
    pub mutex_init: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Tear down a non-recursive mutex.
    pub mutex_drop: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Lock a non-recursive mutex; block if held.
    pub mutex_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Try to lock; return non-zero immediately if the mutex is held.
    pub mutex_try_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Unlock a non-recursive mutex held by the calling thread.
    pub mutex_unlock: unsafe extern "C" fn(m: *mut c_void) -> i8,

    /// Initialise a *recursive* mutex (same-thread re-entrancy
    /// permitted). Required by zenoh-pico.
    pub mutex_rec_init: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Tear down a recursive mutex.
    pub mutex_rec_drop: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Lock a recursive mutex. Re-entry from the owning thread must
    /// succeed without deadlock.
    pub mutex_rec_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Try to lock a recursive mutex; same re-entry semantics as
    /// `mutex_rec_lock`.
    pub mutex_rec_try_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    /// Unlock a recursive mutex; only releases when the lock count
    /// returns to zero.
    pub mutex_rec_unlock: unsafe extern "C" fn(m: *mut c_void) -> i8,

    /// Initialise a condition variable in the caller-provided storage.
    pub condvar_init: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    /// Tear down a condition variable.
    pub condvar_drop: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    /// Wake one waiter on the condition variable.
    pub condvar_signal: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    /// Wake all waiters on the condition variable.
    pub condvar_signal_all: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    /// Atomically release `m` and block on `cv`. The mutex is
    /// re-acquired before this function returns.
    pub condvar_wait: unsafe extern "C" fn(cv: *mut c_void, m: *mut c_void) -> i8,
    /// Like `condvar_wait`, but with an absolute monotonic deadline
    /// (in `clock_ms` units). Returns non-zero on timeout.
    pub condvar_wait_until:
        unsafe extern "C" fn(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8,
}

// ============================================================================
// Registration
// ============================================================================

static VTABLE: AtomicPtr<NrosPlatformVtable> = AtomicPtr::new(core::ptr::null_mut());

/// Register a platform vtable.
///
/// # Safety
///
/// The vtable pointer must remain valid for the lifetime of the program.
/// All function pointers in the vtable must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_platform_cffi_register(vtable: *const NrosPlatformVtable) -> i32 {
    VTABLE.store(vtable as *mut NrosPlatformVtable, Ordering::Release);
    0
}

fn get_vtable() -> &'static NrosPlatformVtable {
    let ptr = VTABLE.load(Ordering::Acquire);
    assert!(!ptr.is_null(), "nros_platform_cffi_register() not called");
    // SAFETY: Registration ensures the pointer is valid and 'static.
    unsafe { &*ptr }
}

// ============================================================================
// CffiPlatform — implements all traits by dispatching through the vtable
// ============================================================================

/// Zero-sized type that implements platform traits via a registered C vtable.
pub struct CffiPlatform;

// We can't directly depend on nros-platform (circular), so the trait impls
// are written in nros-platform's resolve module via a wrapper, or the shim
// crates call these methods directly.

impl nros_platform_api::PlatformClock for CffiPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        unsafe { (get_vtable().clock_ms)() }
    }

    #[inline]
    fn clock_us() -> u64 {
        unsafe { (get_vtable().clock_us)() }
    }
}

impl nros_platform_api::PlatformAlloc for CffiPlatform {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        unsafe { (get_vtable().alloc)(size) }
    }

    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        unsafe { (get_vtable().realloc)(ptr, size) }
    }

    #[inline]
    fn dealloc(ptr: *mut c_void) {
        unsafe { (get_vtable().dealloc)(ptr) }
    }
}

impl nros_platform_api::PlatformSleep for CffiPlatform {
    #[inline]
    fn sleep_us(us: usize) {
        unsafe { (get_vtable().sleep_us)(us) }
    }

    #[inline]
    fn sleep_ms(ms: usize) {
        unsafe { (get_vtable().sleep_ms)(ms) }
    }

    #[inline]
    fn sleep_s(s: usize) {
        unsafe { (get_vtable().sleep_s)(s) }
    }
}

impl nros_platform_api::PlatformYield for CffiPlatform {
    #[inline]
    fn yield_now() {
        unsafe { (get_vtable().yield_now)() }
    }
}

impl nros_platform_api::PlatformRandom for CffiPlatform {
    #[inline]
    fn random_u8() -> u8 {
        unsafe { (get_vtable().random_u8)() }
    }

    #[inline]
    fn random_u16() -> u16 {
        unsafe { (get_vtable().random_u16)() }
    }

    #[inline]
    fn random_u32() -> u32 {
        unsafe { (get_vtable().random_u32)() }
    }

    #[inline]
    fn random_u64() -> u64 {
        unsafe { (get_vtable().random_u64)() }
    }

    #[inline]
    fn random_fill(buf: *mut c_void, len: usize) {
        unsafe { (get_vtable().random_fill)(buf, len) }
    }
}

impl nros_platform_api::PlatformTime for CffiPlatform {
    #[inline]
    fn time_now_ms() -> u64 {
        unsafe { (get_vtable().time_now_ms)() }
    }

    #[inline]
    fn time_since_epoch_secs() -> u32 {
        unsafe { (get_vtable().time_since_epoch_secs)() }
    }

    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        unsafe { (get_vtable().time_since_epoch_nanos)() }
    }
}

impl CffiPlatform {
    // -- Threading --
    #[inline]
    pub fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        unsafe { (get_vtable().task_init)(task, attr, entry, arg) }
    }

    #[inline]
    pub fn task_join(task: *mut c_void) -> i8 {
        unsafe { (get_vtable().task_join)(task) }
    }

    #[inline]
    pub fn task_detach(task: *mut c_void) -> i8 {
        unsafe { (get_vtable().task_detach)(task) }
    }

    #[inline]
    pub fn task_cancel(task: *mut c_void) -> i8 {
        unsafe { (get_vtable().task_cancel)(task) }
    }

    #[inline]
    pub fn task_exit() {
        unsafe { (get_vtable().task_exit)() }
    }

    #[inline]
    pub fn task_free(task: *mut *mut c_void) {
        unsafe { (get_vtable().task_free)(task) }
    }

    #[inline]
    pub fn mutex_init(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_init)(m) }
    }

    #[inline]
    pub fn mutex_drop(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_drop)(m) }
    }

    #[inline]
    pub fn mutex_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_lock)(m) }
    }

    #[inline]
    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_try_lock)(m) }
    }

    #[inline]
    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_unlock)(m) }
    }

    #[inline]
    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_init)(m) }
    }

    #[inline]
    pub fn mutex_rec_drop(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_drop)(m) }
    }

    #[inline]
    pub fn mutex_rec_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_lock)(m) }
    }

    #[inline]
    pub fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_try_lock)(m) }
    }

    #[inline]
    pub fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_unlock)(m) }
    }

    #[inline]
    pub fn condvar_init(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_init)(cv) }
    }

    #[inline]
    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_drop)(cv) }
    }

    #[inline]
    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_signal)(cv) }
    }

    #[inline]
    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_signal_all)(cv) }
    }

    #[inline]
    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_wait)(cv, m) }
    }

    #[inline]
    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        unsafe { (get_vtable().condvar_wait_until)(cv, m, abstime) }
    }
}

impl nros_platform_api::PlatformThreading for CffiPlatform {
    fn task_init(
        task: *mut core::ffi::c_void,
        attr: *mut core::ffi::c_void,
        entry: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>,
        arg: *mut core::ffi::c_void,
    ) -> i8 {
        Self::task_init(task, attr, entry, arg)
    }
    fn task_join(task: *mut core::ffi::c_void) -> i8 {
        Self::task_join(task)
    }
    fn task_detach(task: *mut core::ffi::c_void) -> i8 {
        Self::task_detach(task)
    }
    fn task_cancel(task: *mut core::ffi::c_void) -> i8 {
        Self::task_cancel(task)
    }
    fn task_exit() {
        Self::task_exit()
    }
    fn task_free(task: *mut *mut core::ffi::c_void) {
        Self::task_free(task)
    }
    fn mutex_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_init(m)
    }
    fn mutex_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_drop(m)
    }
    fn mutex_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_lock(m)
    }
    fn mutex_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_try_lock(m)
    }
    fn mutex_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_unlock(m)
    }
    fn mutex_rec_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_init(m)
    }
    fn mutex_rec_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_drop(m)
    }
    fn mutex_rec_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_lock(m)
    }
    fn mutex_rec_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_try_lock(m)
    }
    fn mutex_rec_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_unlock(m)
    }
    fn condvar_init(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_init(cv)
    }
    fn condvar_drop(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_drop(cv)
    }
    fn condvar_signal(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal(cv)
    }
    fn condvar_signal_all(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal_all(cv)
    }
    fn condvar_wait(cv: *mut core::ffi::c_void, m: *mut core::ffi::c_void) -> i8 {
        Self::condvar_wait(cv, m)
    }
    fn condvar_wait_until(
        cv: *mut core::ffi::c_void,
        m: *mut core::ffi::c_void,
        abstime: u64,
    ) -> i8 {
        Self::condvar_wait_until(cv, m, abstime)
    }
}
