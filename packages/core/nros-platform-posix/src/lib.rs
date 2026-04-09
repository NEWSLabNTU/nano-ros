//! POSIX platform implementation for nros.
//!
//! Provides all platform capabilities using standard POSIX APIs:
//! `clock_gettime`, `malloc`/`free`, `nanosleep`, `pthread_*`, `/dev/urandom`.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
use core::ptr;

/// Zero-sized type implementing all platform traits via POSIX APIs.
pub struct PosixPlatform;

// ============================================================================
// Clock — clock_gettime(CLOCK_MONOTONIC)
// ============================================================================

fn clock_monotonic() -> libc::timespec {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts
}

impl PosixPlatform {
    pub fn clock_ms() -> u64 {
        let ts = clock_monotonic();
        ts.tv_sec as u64 * 1000 + ts.tv_nsec as u64 / 1_000_000
    }

    pub fn clock_us() -> u64 {
        let ts = clock_monotonic();
        ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1_000
    }
}

// ============================================================================
// Alloc — system malloc/realloc/free
// ============================================================================

impl PosixPlatform {
    pub fn alloc(size: usize) -> *mut c_void {
        unsafe { libc::malloc(size) }
    }

    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        unsafe { libc::realloc(ptr, size) }
    }

    pub fn dealloc(ptr: *mut c_void) {
        unsafe { libc::free(ptr) }
    }
}

// ============================================================================
// Sleep — nanosleep
// ============================================================================

impl PosixPlatform {
    pub fn sleep_us(us: usize) {
        let ts = libc::timespec {
            tv_sec: (us / 1_000_000) as libc::time_t,
            tv_nsec: ((us % 1_000_000) * 1_000) as libc::c_long,
        };
        unsafe {
            libc::nanosleep(&ts, ptr::null_mut());
        }
    }

    pub fn sleep_ms(ms: usize) {
        Self::sleep_us(ms * 1_000);
    }

    pub fn sleep_s(s: usize) {
        Self::sleep_us(s * 1_000_000);
    }
}

// ============================================================================
// Random — /dev/urandom via getrandom(2) or read()
// ============================================================================

impl PosixPlatform {
    fn fill_random(buf: *mut u8, len: usize) {
        // Use getrandom(2) on Linux, fall back to /dev/urandom
        #[cfg(target_os = "linux")]
        unsafe {
            libc::getrandom(buf as *mut c_void, len, 0);
        }

        #[cfg(not(target_os = "linux"))]
        unsafe {
            use core::ffi::CStr;
            let fd = libc::open(
                CStr::from_bytes_with_nul_unchecked(b"/dev/urandom\0").as_ptr(),
                libc::O_RDONLY,
            );
            if fd >= 0 {
                libc::read(fd, buf as *mut c_void, len);
                libc::close(fd);
            }
        }
    }

    pub fn random_u8() -> u8 {
        let mut v = 0u8;
        Self::fill_random(&mut v as *mut u8, 1);
        v
    }

    pub fn random_u16() -> u16 {
        let mut v = 0u16;
        Self::fill_random(&mut v as *mut u16 as *mut u8, 2);
        v
    }

    pub fn random_u32() -> u32 {
        let mut v = 0u32;
        Self::fill_random(&mut v as *mut u32 as *mut u8, 4);
        v
    }

    pub fn random_u64() -> u64 {
        let mut v = 0u64;
        Self::fill_random(&mut v as *mut u64 as *mut u8, 8);
        v
    }

    pub fn random_fill(buf: *mut c_void, len: usize) {
        Self::fill_random(buf as *mut u8, len);
    }
}

// ============================================================================
// Time — clock_gettime(CLOCK_REALTIME)
// ============================================================================

impl PosixPlatform {
    pub fn time_now_ms() -> u64 {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);
        }
        ts.tv_sec as u64 * 1000 + ts.tv_nsec as u64 / 1_000_000
    }

    pub fn time_since_epoch_secs() -> u32 {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);
        }
        ts.tv_sec as u32
    }

    pub fn time_since_epoch_nanos() -> u32 {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);
        }
        ts.tv_nsec as u32
    }
}

// ============================================================================
// Threading — pthreads
// ============================================================================

impl PosixPlatform {
    pub fn task_init(
        task: *mut c_void,
        _attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        let entry = match entry {
            Some(f) => f,
            None => return -1,
        };
        // SAFETY: libc expects `extern "C" fn` but zenoh-pico passes
        // `unsafe extern "C" fn`. The ABI is identical; transmute is safe.
        let start_routine: extern "C" fn(*mut c_void) -> *mut c_void =
            unsafe { core::mem::transmute(entry) };
        let ret = unsafe {
            libc::pthread_create(
                task as *mut libc::pthread_t,
                ptr::null(),
                start_routine,
                arg,
            )
        };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_join(task: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_join(*(task as *const libc::pthread_t), ptr::null_mut()) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_detach(task: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_detach(*(task as *const libc::pthread_t)) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_cancel(task: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_cancel(*(task as *const libc::pthread_t)) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_exit() {
        unsafe { libc::pthread_exit(ptr::null_mut()) }
    }

    pub fn task_free(_task: *mut *mut c_void) {
        // pthread handles don't need explicit freeing
    }

    // -- Mutex --

    pub fn mutex_init(m: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_mutex_init(m as *mut libc::pthread_mutex_t, ptr::null()) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_drop(m: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_mutex_destroy(m as *mut libc::pthread_mutex_t) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_lock(m: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_mutex_lock(m as *mut libc::pthread_mutex_t) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_mutex_trylock(m as *mut libc::pthread_mutex_t) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_mutex_unlock(m as *mut libc::pthread_mutex_t) };
        if ret == 0 { 0 } else { -1 }
    }

    // -- Recursive mutex --

    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        unsafe {
            let mut attr: libc::pthread_mutexattr_t = core::mem::zeroed();
            libc::pthread_mutexattr_init(&mut attr);
            libc::pthread_mutexattr_settype(&mut attr, libc::PTHREAD_MUTEX_RECURSIVE);
            let ret = libc::pthread_mutex_init(m as *mut libc::pthread_mutex_t, &attr);
            libc::pthread_mutexattr_destroy(&mut attr);
            if ret == 0 { 0 } else { -1 }
        }
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

    // -- Condvar --

    pub fn condvar_init(cv: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_cond_init(cv as *mut libc::pthread_cond_t, ptr::null()) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_cond_destroy(cv as *mut libc::pthread_cond_t) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_cond_signal(cv as *mut libc::pthread_cond_t) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        let ret = unsafe { libc::pthread_cond_broadcast(cv as *mut libc::pthread_cond_t) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        let ret = unsafe {
            libc::pthread_cond_wait(
                cv as *mut libc::pthread_cond_t,
                m as *mut libc::pthread_mutex_t,
            )
        };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime_ms: u64) -> i8 {
        let ts = libc::timespec {
            tv_sec: (abstime_ms / 1000) as libc::time_t,
            tv_nsec: ((abstime_ms % 1000) * 1_000_000) as libc::c_long,
        };
        let ret = unsafe {
            libc::pthread_cond_timedwait(
                cv as *mut libc::pthread_cond_t,
                m as *mut libc::pthread_mutex_t,
                &ts,
            )
        };
        if ret == 0 { 0 } else { -1 }
    }
}
