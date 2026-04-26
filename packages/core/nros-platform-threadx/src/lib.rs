//! nros platform implementation for ThreadX RTOS.
//!
//! Calls ThreadX C APIs (`tx_thread_create`, `tx_byte_allocate`, `tx_mutex_*`,
//! etc.) via `extern "C"` declarations resolved at link time from the ThreadX
//! kernel library linked by the board crate.
//!
//! # Memory
//!
//! ThreadX uses byte pools for allocation. The board crate must call
//! `set_byte_pool()` before any allocations to register the pool pointer.
//!
//! # Condition variables
//!
//! ThreadX has no native condvar. Emulated with `tx_semaphore_*`.

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, Ordering};
#[allow(unused_imports)]
use nros_platform_api::{PlatformAlloc, PlatformClock};

mod ffi;
pub mod net;

/// Zero-sized type implementing all platform methods for ThreadX.
pub struct ThreadxPlatform;

// ============================================================================
// Byte pool registration (board crate must call this at init)
// ============================================================================

static BYTE_POOL: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

/// Register the ThreadX byte pool for memory allocation.
/// Must be called by the board crate before any alloc calls.
pub fn set_byte_pool(pool: *mut c_void) {
    BYTE_POOL.store(pool, Ordering::Release);
}

/// C-callable version for app_define.c to call during ThreadX init.
#[unsafe(no_mangle)]
pub extern "C" fn nros_platform_threadx_set_byte_pool(pool: *mut c_void) {
    set_byte_pool(pool);
}

// ============================================================================
// Clock — tx_time_get
// ============================================================================

impl nros_platform_api::PlatformClock for ThreadxPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        // ThreadX ticks — assumes 100 ticks/sec (TX_TIMER_TICKS_PER_SECOND=100)
        // Board crates can adjust if tick rate differs.
        let ticks = unsafe { ffi::tx_time_get() };
        ticks as u64 * 10 // 100 Hz → ms
    }

    #[inline]
    fn clock_us() -> u64 {
        <Self as nros_platform_api::PlatformClock>::clock_ms() * 1000
    }
}

// ============================================================================
// Memory — tx_byte_allocate / tx_byte_release
// ============================================================================

impl nros_platform_api::PlatformAlloc for ThreadxPlatform {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        let pool = BYTE_POOL.load(Ordering::Acquire);
        if pool.is_null() {
            return core::ptr::null_mut();
        }
        let mut ptr: *mut c_void = core::ptr::null_mut();
        let ret =
            unsafe { ffi::tx_byte_allocate(pool, &mut ptr, size as u32, ffi::TX_WAIT_FOREVER) };
        if ret == ffi::TX_SUCCESS {
            ptr
        } else {
            core::ptr::null_mut()
        }
    }

    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        use nros_platform_api::PlatformAlloc;
        if ptr.is_null() {
            return <Self as PlatformAlloc>::alloc(size);
        }
        let new_ptr = <Self as PlatformAlloc>::alloc(size);
        if !new_ptr.is_null() {
            unsafe { core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, size) };
            <Self as PlatformAlloc>::dealloc(ptr);
        }
        new_ptr
    }

    #[inline]
    fn dealloc(ptr: *mut c_void) {
        if !ptr.is_null() {
            unsafe { ffi::tx_byte_release(ptr) };
        }
    }
}

// ============================================================================
// Sleep — tx_thread_sleep
// ============================================================================

impl nros_platform_api::PlatformSleep for ThreadxPlatform {
    #[inline]
    fn sleep_us(us: usize) {
        use nros_platform_api::PlatformSleep;
        <Self as PlatformSleep>::sleep_ms(us.div_ceil(1000));
    }

    #[inline]
    fn sleep_ms(ms: usize) {
        // Convert ms to ticks (100 Hz = 10ms per tick)
        let ticks = (ms as u32).div_ceil(10);
        unsafe { ffi::tx_thread_sleep(ticks) };
    }

    #[inline]
    fn sleep_s(s: usize) {
        use nros_platform_api::PlatformSleep;
        <Self as PlatformSleep>::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Yield — tx_thread_relinquish
// ============================================================================

impl nros_platform_api::PlatformYield for ThreadxPlatform {
    #[inline]
    fn yield_now() {
        // `tx_thread_relinquish` is ThreadX's native cooperative yield:
        // move the current thread to the end of its priority ready list
        // and run the scheduler. Not ISR-safe.
        unsafe { ffi::tx_thread_relinquish() };
    }
}

// ============================================================================
// Random — xorshift (shared helpers from nros_platform_api::xorshift32)
// ============================================================================

static mut RNG_STATE: u32 = nros_platform_api::xorshift32::DEFAULT_SEED;

pub fn seed(value: u32) {
    unsafe { nros_platform_api::xorshift32::seed(&raw mut RNG_STATE, value) }
}

/// C-callable version for app_define.c to seed the platform RNG.
#[unsafe(no_mangle)]
pub extern "C" fn nros_platform_threadx_seed_rng(value: u32) {
    seed(value);
}

fn next_u32() -> u32 {
    unsafe { nros_platform_api::xorshift32::next(&raw mut RNG_STATE) }
}

impl nros_platform_api::PlatformRandom for ThreadxPlatform {
    fn random_u8() -> u8 {
        (next_u32() & 0xFF) as u8
    }
    fn random_u16() -> u16 {
        (next_u32() & 0xFFFF) as u16
    }
    fn random_u32() -> u32 {
        next_u32()
    }
    fn random_u64() -> u64 {
        ((next_u32() as u64) << 32) | next_u32() as u64
    }

    fn random_fill(buf: *mut c_void, len: usize) {
        unsafe {
            nros_platform_api::xorshift32::random_fill(&raw mut RNG_STATE, buf as *mut u8, len)
        }
    }
}

// ============================================================================
// Time — monotonic (no RTC)
// ============================================================================

impl nros_platform_api::PlatformTime for ThreadxPlatform {
    #[inline]
    fn time_now_ms() -> u64 {
        <Self as PlatformClock>::clock_ms()
    }
    #[inline]
    fn time_since_epoch_secs() -> u32 {
        (<Self as PlatformClock>::clock_ms() / 1000) as u32
    }
    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        ((<Self as PlatformClock>::clock_ms() % 1000) * 1_000_000) as u32
    }
}

// ============================================================================
// Threading — ThreadX threads, mutexes, semaphore-based condvars
// ============================================================================

impl ThreadxPlatform {
    pub fn task_init(
        _task: *mut c_void,
        _attr: *mut c_void,
        _entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        _arg: *mut c_void,
    ) -> i8 {
        // ThreadX thread creation requires stack allocation and TX_THREAD struct.
        // This is too complex for the generic interface — board crates handle
        // thread creation directly. Return error here.
        -1
    }

    pub fn task_join(_task: *mut c_void) -> i8 {
        0
    }
    pub fn task_detach(_task: *mut c_void) -> i8 {
        0
    }
    pub fn task_cancel(_task: *mut c_void) -> i8 {
        0
    }
    pub fn task_exit() {}
    pub fn task_free(_task: *mut *mut c_void) {}

    // -- Mutex --

    pub fn mutex_init(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::tx_mutex_create(m, c"nros".as_ptr(), 1) }; // TX_INHERIT=1
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }

    pub fn mutex_drop(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::tx_mutex_delete(m) };
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }

    pub fn mutex_lock(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::tx_mutex_get(m, ffi::TX_WAIT_FOREVER) };
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }

    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::tx_mutex_get(m, ffi::TX_NO_WAIT) };
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }

    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::tx_mutex_put(m) };
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }

    // -- Recursive mutex (ThreadX mutexes are inherently recursive) --

    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        Self::mutex_init(m)
    }
    pub fn mutex_rec_drop(m: *mut c_void) -> i8 {
        Self::mutex_drop(m)
    }
    pub fn mutex_rec_lock(m: *mut c_void) -> i8 {
        Self::mutex_lock(m)
    }
    pub fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        Self::mutex_try_lock(m)
    }
    pub fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        Self::mutex_unlock(m)
    }

    // -- Condition variables (semaphore-based emulation) --

    pub fn condvar_init(cv: *mut c_void) -> i8 {
        let ret = unsafe { ffi::tx_semaphore_create(cv, c"nros_cv".as_ptr(), 0) };
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }

    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        let ret = unsafe { ffi::tx_semaphore_delete(cv) };
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }

    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        unsafe { ffi::tx_semaphore_put(cv) };
        0
    }

    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        Self::condvar_signal(cv)
    }

    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        Self::mutex_unlock(m);
        unsafe { ffi::tx_semaphore_get(cv, ffi::TX_WAIT_FOREVER) };
        Self::mutex_lock(m);
        0
    }

    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        let now = Self::clock_ms();
        let timeout_ms = abstime.saturating_sub(now);
        let timeout_ticks = (timeout_ms as u32).div_ceil(10); // 100 Hz
        Self::mutex_unlock(m);
        let ret = unsafe { ffi::tx_semaphore_get(cv, timeout_ticks) };
        Self::mutex_lock(m);
        if ret == ffi::TX_SUCCESS { 0 } else { -1 }
    }
}

impl nros_platform_api::PlatformThreading for ThreadxPlatform {
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
