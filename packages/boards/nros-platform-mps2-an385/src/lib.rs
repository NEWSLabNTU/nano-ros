//! nros platform implementation for QEMU MPS2-AN385 bare-metal.
//!
//! Provides all platform primitives (clock, memory, random, sleep, time,
//! threading stubs, libc stubs) for the MPS2-AN385 board. Used by both
//! zpico-platform-shim and xrce-platform-shim via the nros-platform
//! `ConcretePlatform` type alias.
//!
//! This crate has **no nros dependency** — it only provides the hardware
//! abstraction needed by the unified platform interface.

#![no_std]

pub mod clock;
pub mod libc_stubs;
pub mod memory;
pub mod net;
pub mod random;
pub mod sleep;
pub mod threading;
pub mod time;
pub mod timing;

/// Zero-sized type implementing all platform methods for MPS2-AN385.
///
/// Methods match the signatures expected by zpico-platform-shim and
/// xrce-platform-shim (delegating to the hardware-specific modules).
pub struct Mps2An385Platform;

// ============================================================================
// Clock
// ============================================================================

impl Mps2An385Platform {
    #[inline]
    pub fn clock_ms() -> u64 {
        clock::clock_ms()
    }

    #[inline]
    pub fn clock_us() -> u64 {
        clock::clock_ms() * 1000
    }
}

// ============================================================================
// Memory
// ============================================================================

impl Mps2An385Platform {
    #[inline]
    pub fn alloc(size: usize) -> *mut core::ffi::c_void {
        memory::alloc(size)
    }

    #[inline]
    pub fn realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void {
        memory::realloc(ptr, size)
    }

    #[inline]
    pub fn dealloc(ptr: *mut core::ffi::c_void) {
        memory::dealloc(ptr)
    }
}

// ============================================================================
// Sleep
// ============================================================================

impl Mps2An385Platform {
    #[inline]
    pub fn sleep_us(us: usize) {
        sleep::sleep_ms(us.div_ceil(1000));
    }

    #[inline]
    pub fn sleep_ms(ms: usize) {
        sleep::sleep_ms(ms);
    }

    #[inline]
    pub fn sleep_s(s: usize) {
        sleep::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Random
// ============================================================================

impl Mps2An385Platform {
    #[inline]
    pub fn random_u8() -> u8 {
        random::random_u8()
    }

    #[inline]
    pub fn random_u16() -> u16 {
        random::random_u16()
    }

    #[inline]
    pub fn random_u32() -> u32 {
        random::random_u32()
    }

    #[inline]
    pub fn random_u64() -> u64 {
        random::random_u64()
    }

    #[inline]
    pub fn random_fill(buf: *mut core::ffi::c_void, len: usize) {
        random::random_fill(buf, len)
    }
}

// ============================================================================
// Time
// ============================================================================

impl Mps2An385Platform {
    #[inline]
    pub fn time_now_ms() -> u64 {
        clock::clock_ms()
    }

    #[inline]
    pub fn time_since_epoch_secs() -> u32 {
        (clock::clock_ms() / 1000) as u32
    }

    #[inline]
    pub fn time_since_epoch_nanos() -> u32 {
        ((clock::clock_ms() % 1000) * 1_000_000) as u32
    }
}

// ============================================================================
// Threading (single-threaded bare-metal — all no-ops)
// ============================================================================

impl Mps2An385Platform {
    pub fn task_init(
        _task: *mut core::ffi::c_void,
        _attr: *mut core::ffi::c_void,
        _entry: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>,
        _arg: *mut core::ffi::c_void,
    ) -> i8 {
        -1 // Cannot create threads on single-threaded platform
    }

    pub fn task_join(_task: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_detach(_task: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_cancel(_task: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_exit() {}
    pub fn task_free(_task: *mut *mut core::ffi::c_void) {}

    pub fn mutex_init(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_drop(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_try_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_unlock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }

    pub fn mutex_rec_init(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_drop(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_try_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_unlock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }

    pub fn condvar_init(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_drop(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_signal(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_signal_all(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_wait(_cv: *mut core::ffi::c_void, _m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_wait_until(
        _cv: *mut core::ffi::c_void,
        _m: *mut core::ffi::c_void,
        _abstime: u64,
    ) -> i8 {
        0
    }
}
