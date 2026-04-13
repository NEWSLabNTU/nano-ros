//! Zephyr + POSIX subsystem C API declarations.

#![allow(dead_code, non_camel_case_types)]

use core::ffi::c_void;

// ---- Constants ----

/// `PTHREAD_MUTEX_RECURSIVE` — value 1 in Zephyr POSIX, glibc, and musl.
pub const PTHREAD_MUTEX_RECURSIVE: i32 = 1;

/// `CLOCK_MONOTONIC` — value 1 in Zephyr POSIX and Linux.
pub const CLOCK_MONOTONIC: i32 = 1;

/// `ETIMEDOUT` — value 110 in Zephyr POSIX and Linux (`errno.h`).
pub const ETIMEDOUT: i32 = 110;

// ---- struct timespec ----
//
// Layout matches <time.h> on Zephyr POSIX and Linux. tv_sec is time_t
// (int64_t on 64-bit platforms, int64_t on recent Zephyr POSIX). tv_nsec is
// long. We use i64 for both — on 32-bit Zephyr tv_nsec is int32 but the
// ABI passes a struct by reference so the extra width is harmless as long
// as we construct the struct ourselves (we do).
#[repr(C)]
pub struct timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

unsafe extern "C" {
    // ---- Kernel (real symbols, callable directly) ----

    /// `int32_t k_usleep(int32_t us)` — returns remaining time if interrupted.
    pub fn k_usleep(us: i32) -> i32;

    /// `void *k_malloc(size_t size)` — kernel heap allocation.
    pub fn k_malloc(size: usize) -> *mut c_void;

    /// `void k_free(void *ptr)` — kernel heap free.
    pub fn k_free(ptr: *mut c_void);

    /// `uint32_t sys_rand32_get(void)`.
    pub fn sys_rand32_get() -> u32;

    // ---- Kernel (static inline in headers — wrapped by
    //      zephyr/nros_platform_zephyr_shims.c) ----

    /// Real-symbol wrapper around `k_uptime_get()`.
    pub fn nros_zephyr_uptime_ms() -> i64;

    /// Real-symbol wrapper around `k_msleep(ms)`.
    pub fn nros_zephyr_msleep(ms: i32) -> i32;

    /// Real-symbol wrapper around `sys_rand_get(dst, len)`.
    pub fn nros_zephyr_rand_fill(dst: *mut c_void, len: usize);

    // ---- POSIX: threads ----

    pub fn pthread_create(
        thread: *mut c_void,
        attr: *const c_void,
        start_routine: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> i32;

    pub fn pthread_join(thread: u32, retval: *mut *mut c_void) -> i32;
    pub fn pthread_detach(thread: u32) -> i32;
    pub fn pthread_cancel(thread: u32) -> i32;
    pub fn pthread_exit(retval: *mut c_void) -> !;

    // ---- POSIX: mutex ----

    pub fn pthread_mutex_init(m: *mut c_void, attr: *const c_void) -> i32;
    pub fn pthread_mutex_destroy(m: *mut c_void) -> i32;
    pub fn pthread_mutex_lock(m: *mut c_void) -> i32;
    pub fn pthread_mutex_trylock(m: *mut c_void) -> i32;
    pub fn pthread_mutex_unlock(m: *mut c_void) -> i32;

    pub fn pthread_mutexattr_init(attr: *mut c_void) -> i32;
    pub fn pthread_mutexattr_destroy(attr: *mut c_void) -> i32;
    pub fn pthread_mutexattr_settype(attr: *mut c_void, kind: i32) -> i32;

    // ---- POSIX: condvar ----

    pub fn pthread_cond_init(cv: *mut c_void, attr: *const c_void) -> i32;
    pub fn pthread_cond_destroy(cv: *mut c_void) -> i32;
    pub fn pthread_cond_signal(cv: *mut c_void) -> i32;
    pub fn pthread_cond_broadcast(cv: *mut c_void) -> i32;
    pub fn pthread_cond_wait(cv: *mut c_void, m: *mut c_void) -> i32;
    pub fn pthread_cond_timedwait(cv: *mut c_void, m: *mut c_void, abstime: *const timespec)
    -> i32;

    pub fn pthread_condattr_init(attr: *mut c_void) -> i32;
    pub fn pthread_condattr_destroy(attr: *mut c_void) -> i32;
    pub fn pthread_condattr_setclock(attr: *mut c_void, clock_id: i32) -> i32;
}
