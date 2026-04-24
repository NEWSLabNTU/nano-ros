//! nros platform implementation for NuttX RTOS.
//!
//! NuttX is POSIX-compatible for system primitives (clock, alloc, sleep,
//! random, threading) — these delegate to `PosixPlatform`.
//!
//! Networking uses NuttX-specific constants via `nuttx-sys` (bindgen)
//! because NuttX socket constants differ from Linux (SOL_SOCKET=1,
//! SO_RCVTIMEO=10, TCP_NODELAY=16, O_NONBLOCK=64, SHUT_RDWR=3).

#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod net;

use core::ffi::c_void;
use nros_platform_api::{
    PlatformAlloc, PlatformClock, PlatformRandom, PlatformSleep, PlatformTime, PlatformYield,
};
use nros_platform_posix::PosixPlatform;

/// NuttX platform type.
///
/// System primitives delegate to PosixPlatform. Networking uses
/// NuttX-specific types from nuttx-sys.
pub struct NuttxPlatform;

// ============================================================================
// System primitives — delegate to PosixPlatform
// ============================================================================

impl PlatformClock for NuttxPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        PosixPlatform::clock_ms()
    }
    #[inline]
    fn clock_us() -> u64 {
        PosixPlatform::clock_us()
    }
}

impl PlatformAlloc for NuttxPlatform {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        PosixPlatform::alloc(size)
    }
    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        PosixPlatform::realloc(ptr, size)
    }
    #[inline]
    fn dealloc(ptr: *mut c_void) {
        PosixPlatform::dealloc(ptr)
    }
}

impl PlatformSleep for NuttxPlatform {
    #[inline]
    fn sleep_us(us: usize) {
        PosixPlatform::sleep_us(us)
    }
    #[inline]
    fn sleep_ms(ms: usize) {
        PosixPlatform::sleep_ms(ms)
    }
    #[inline]
    fn sleep_s(s: usize) {
        PosixPlatform::sleep_s(s)
    }
}

impl PlatformYield for NuttxPlatform {
    #[inline]
    fn yield_now() {
        // NuttX is POSIX-compliant — `sched_yield(2)` is the native
        // cooperative yield and the smallest primitive that matches
        // the `socket_wait_event` intent.
        PosixPlatform::yield_now()
    }
}

impl PlatformRandom for NuttxPlatform {
    #[inline]
    fn random_u8() -> u8 {
        PosixPlatform::random_u8()
    }
    #[inline]
    fn random_u16() -> u16 {
        PosixPlatform::random_u16()
    }
    #[inline]
    fn random_u32() -> u32 {
        PosixPlatform::random_u32()
    }
    #[inline]
    fn random_u64() -> u64 {
        PosixPlatform::random_u64()
    }
    #[inline]
    fn random_fill(buf: *mut c_void, len: usize) {
        PosixPlatform::random_fill(buf, len)
    }
}

impl PlatformTime for NuttxPlatform {
    #[inline]
    fn time_now_ms() -> u64 {
        PosixPlatform::time_now_ms()
    }
    #[inline]
    fn time_since_epoch_secs() -> u32 {
        PosixPlatform::time_since_epoch_secs()
    }
    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        PosixPlatform::time_since_epoch_nanos()
    }
}

impl NuttxPlatform {
    #[inline]
    pub fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        PosixPlatform::task_init(task, attr, entry, arg)
    }
    #[inline]
    pub fn task_join(task: *mut c_void) -> i8 {
        PosixPlatform::task_join(task)
    }
    #[inline]
    pub fn task_detach(task: *mut c_void) -> i8 {
        PosixPlatform::task_detach(task)
    }
    #[inline]
    pub fn task_cancel(task: *mut c_void) -> i8 {
        PosixPlatform::task_cancel(task)
    }
    #[inline]
    pub fn task_exit() {
        PosixPlatform::task_exit()
    }
    #[inline]
    pub fn task_free(task: *mut *mut c_void) {
        PosixPlatform::task_free(task)
    }
    #[inline]
    pub fn mutex_init(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_init(m)
    }
    #[inline]
    pub fn mutex_drop(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_drop(m)
    }
    #[inline]
    pub fn mutex_lock(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_lock(m)
    }
    #[inline]
    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_try_lock(m)
    }
    #[inline]
    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_unlock(m)
    }
    #[inline]
    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_rec_init(m)
    }
    #[inline]
    pub fn mutex_rec_drop(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_rec_drop(m)
    }
    #[inline]
    pub fn mutex_rec_lock(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_rec_lock(m)
    }
    #[inline]
    pub fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_rec_try_lock(m)
    }
    #[inline]
    pub fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        PosixPlatform::mutex_rec_unlock(m)
    }
    #[inline]
    pub fn condvar_init(cv: *mut c_void) -> i8 {
        PosixPlatform::condvar_init(cv)
    }
    #[inline]
    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        PosixPlatform::condvar_drop(cv)
    }
    #[inline]
    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        PosixPlatform::condvar_signal(cv)
    }
    #[inline]
    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        PosixPlatform::condvar_signal_all(cv)
    }
    #[inline]
    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        PosixPlatform::condvar_wait(cv, m)
    }
    #[inline]
    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime_ms: u64) -> i8 {
        PosixPlatform::condvar_wait_until(cv, m, abstime_ms)
    }
}

impl nros_platform_api::PlatformThreading for NuttxPlatform {
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
