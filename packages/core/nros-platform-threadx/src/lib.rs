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

mod ffi;

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

// ============================================================================
// Clock — tx_time_get
// ============================================================================

impl ThreadxPlatform {
    #[inline]
    pub fn clock_ms() -> u64 {
        // ThreadX ticks — assumes 100 ticks/sec (TX_TIMER_TICKS_PER_SECOND=100)
        // Board crates can adjust if tick rate differs.
        let ticks = unsafe { ffi::tx_time_get() };
        ticks as u64 * 10 // 100 Hz → ms
    }

    #[inline]
    pub fn clock_us() -> u64 {
        Self::clock_ms() * 1000
    }
}

// ============================================================================
// Memory — tx_byte_allocate / tx_byte_release
// ============================================================================

impl ThreadxPlatform {
    #[inline]
    pub fn alloc(size: usize) -> *mut c_void {
        let pool = BYTE_POOL.load(Ordering::Acquire);
        if pool.is_null() {
            return core::ptr::null_mut();
        }
        let mut ptr: *mut c_void = core::ptr::null_mut();
        let ret = unsafe { ffi::tx_byte_allocate(pool, &mut ptr, size as u32, ffi::TX_NO_WAIT) };
        if ret == ffi::TX_SUCCESS {
            ptr
        } else {
            core::ptr::null_mut()
        }
    }

    #[inline]
    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        if ptr.is_null() {
            return Self::alloc(size);
        }
        let new_ptr = Self::alloc(size);
        if !new_ptr.is_null() {
            unsafe { core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, size) };
            Self::dealloc(ptr);
        }
        new_ptr
    }

    #[inline]
    pub fn dealloc(ptr: *mut c_void) {
        if !ptr.is_null() {
            unsafe { ffi::tx_byte_release(ptr) };
        }
    }
}

// ============================================================================
// Sleep — tx_thread_sleep
// ============================================================================

impl ThreadxPlatform {
    #[inline]
    pub fn sleep_us(us: usize) {
        Self::sleep_ms(us.div_ceil(1000));
    }

    #[inline]
    pub fn sleep_ms(ms: usize) {
        // Convert ms to ticks (100 Hz = 10ms per tick)
        let ticks = (ms as u32).div_ceil(10);
        unsafe { ffi::tx_thread_sleep(ticks) };
    }

    #[inline]
    pub fn sleep_s(s: usize) {
        Self::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Random — xorshift (same as bare-metal)
// ============================================================================

static mut RNG_STATE: u32 = 0x12345678;

pub fn seed(value: u32) {
    unsafe { RNG_STATE = if value == 0 { 0x12345678 } else { value } }
}

fn next_u32() -> u32 {
    unsafe {
        let mut x = RNG_STATE;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        RNG_STATE = x;
        x
    }
}

impl ThreadxPlatform {
    pub fn random_u8() -> u8 {
        (next_u32() & 0xFF) as u8
    }
    pub fn random_u16() -> u16 {
        (next_u32() & 0xFFFF) as u16
    }
    pub fn random_u32() -> u32 {
        next_u32()
    }
    pub fn random_u64() -> u64 {
        ((next_u32() as u64) << 32) | next_u32() as u64
    }

    pub fn random_fill(buf: *mut c_void, len: usize) {
        if buf.is_null() {
            return;
        }
        let ptr = buf as *mut u8;
        let mut offset = 0;
        let mut remaining = len;
        while remaining >= 4 {
            let bytes = next_u32().to_ne_bytes();
            unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), 4) };
            offset += 4;
            remaining -= 4;
        }
        if remaining > 0 {
            let bytes = next_u32().to_ne_bytes();
            unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), remaining) };
        }
    }
}

// ============================================================================
// Time — monotonic (no RTC)
// ============================================================================

impl ThreadxPlatform {
    #[inline]
    pub fn time_now_ms() -> u64 {
        Self::clock_ms()
    }
    #[inline]
    pub fn time_since_epoch_secs() -> u32 {
        (Self::clock_ms() / 1000) as u32
    }
    #[inline]
    pub fn time_since_epoch_nanos() -> u32 {
        ((Self::clock_ms() % 1000) * 1_000_000) as u32
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
