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
use nros_platform_posix::PosixPlatform;

/// NuttX platform type.
///
/// System primitives delegate to PosixPlatform. Networking uses
/// NuttX-specific types from nuttx-sys.
pub struct NuttxPlatform;

// ============================================================================
// System primitives — delegate to PosixPlatform
// ============================================================================

impl NuttxPlatform {
    #[inline]
    pub fn clock_ms() -> u64 {
        PosixPlatform::clock_ms()
    }
    #[inline]
    pub fn clock_us() -> u64 {
        PosixPlatform::clock_us()
    }
    #[inline]
    pub fn alloc(size: usize) -> *mut c_void {
        PosixPlatform::alloc(size)
    }
    #[inline]
    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        PosixPlatform::realloc(ptr, size)
    }
    #[inline]
    pub fn dealloc(ptr: *mut c_void) {
        PosixPlatform::dealloc(ptr)
    }
    #[inline]
    pub fn sleep_us(us: usize) {
        PosixPlatform::sleep_us(us)
    }
    #[inline]
    pub fn sleep_ms(ms: usize) {
        PosixPlatform::sleep_ms(ms)
    }
    #[inline]
    pub fn sleep_s(s: usize) {
        PosixPlatform::sleep_s(s)
    }
    #[inline]
    pub fn random_u8() -> u8 {
        PosixPlatform::random_u8()
    }
    #[inline]
    pub fn random_u16() -> u16 {
        PosixPlatform::random_u16()
    }
    #[inline]
    pub fn random_u32() -> u32 {
        PosixPlatform::random_u32()
    }
    #[inline]
    pub fn random_u64() -> u64 {
        PosixPlatform::random_u64()
    }
    #[inline]
    pub fn random_fill(buf: *mut c_void, len: usize) {
        PosixPlatform::random_fill(buf, len)
    }
    #[inline]
    pub fn time_now_ms() -> u64 {
        PosixPlatform::time_now_ms()
    }
    #[inline]
    pub fn time_since_epoch_secs() -> u32 {
        PosixPlatform::time_since_epoch_secs()
    }
    #[inline]
    pub fn time_since_epoch_nanos() -> u32 {
        PosixPlatform::time_since_epoch_nanos()
    }
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
