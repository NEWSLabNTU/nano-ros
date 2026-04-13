//! nros platform implementation for Zephyr RTOS.
//!
//! Replaces `zenoh-pico/src/system/zephyr/system.c`. Called through
//! `zpico-platform-shim` → `ConcretePlatform`.
//!
//! # Dependencies
//!
//! Relies on two layers provided by Zephyr's build system:
//!
//! - **Zephyr kernel** (`k_uptime_get`, `k_msleep`, `k_malloc`, `k_free`,
//!   `sys_rand32_get`) — always available when `CONFIG_NROS=y`.
//! - **Zephyr POSIX subsystem** (`pthread_*`, `clock_gettime`) — required
//!   because zenoh-pico's `system/platform/zephyr.h` defines
//!   `_z_task_t = pthread_t`, `_z_mutex_t = pthread_mutex_t`,
//!   `_z_condvar_t = pthread_cond_t`. The storage is allocated by
//!   zenoh-pico; we only call the POSIX functions on those pointers.
//!
//! # Clock ABI caveat
//!
//! `zpico-platform-shim` declares `z_clock_now() -> usize`, but on Zephyr
//! `z_clock_t = struct timespec` (16 bytes). This is the same latent ABI
//! mismatch that exists on the POSIX backend today. Call-sites that compare
//! clocks via `z_clock_elapsed_ms` still work in practice because
//! `z_clock_ms()` is interpreted as elapsed milliseconds. Fixing this
//! properly requires changing the shim signatures across all backends
//! (not in scope for this crate).

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

mod ffi;

/// Zero-sized type implementing all platform methods for Zephyr.
pub struct ZephyrPlatform;

// ============================================================================
// Clock — k_uptime_get (monotonic, milliseconds since boot)
// ============================================================================

impl ZephyrPlatform {
    #[inline]
    pub fn clock_ms() -> u64 {
        // k_uptime_get is a static inline in Zephyr headers; we go through
        // a real-symbol wrapper (see zephyr/nros_platform_zephyr_shims.c).
        unsafe { ffi::nros_zephyr_uptime_ms() as u64 }
    }

    #[inline]
    pub fn clock_us() -> u64 {
        // Zephyr's sub-ms precision requires cycle counter; ms * 1000 is fine
        // for zenoh-pico's use of clock_us (protocol timing, not profiling).
        Self::clock_ms() * 1000
    }
}

// ============================================================================
// Memory — k_malloc / k_free from Zephyr's kernel heap
// ============================================================================

impl ZephyrPlatform {
    #[inline]
    pub fn alloc(size: usize) -> *mut c_void {
        unsafe { ffi::k_malloc(size) }
    }

    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        // Zephyr has no k_realloc. Matches zenoh-pico's zephyr/system.c which
        // returned NULL — but that breaks any code that actually uses realloc.
        // Allocate-copy-free instead; caller is responsible for not reading
        // past the old allocation.
        if ptr.is_null() {
            return Self::alloc(size);
        }
        let new_ptr = Self::alloc(size);
        if !new_ptr.is_null() {
            unsafe {
                core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, size);
            }
            Self::dealloc(ptr);
        }
        new_ptr
    }

    #[inline]
    pub fn dealloc(ptr: *mut c_void) {
        unsafe { ffi::k_free(ptr) }
    }
}

// ============================================================================
// Sleep — k_msleep / k_usleep
// ============================================================================

impl ZephyrPlatform {
    pub fn sleep_us(us: usize) {
        // k_usleep's lower bound is one tick; zenoh-pico accepts the rounding.
        let mut rem = us as i32;
        while rem > 0 {
            rem = unsafe { ffi::k_usleep(rem) };
        }
    }

    pub fn sleep_ms(ms: usize) {
        let mut rem = ms as i32;
        while rem > 0 {
            rem = unsafe { ffi::nros_zephyr_msleep(rem) };
        }
    }

    pub fn sleep_s(s: usize) {
        Self::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Random — Zephyr sys_rand32_get / sys_rand_get
// ============================================================================

impl ZephyrPlatform {
    pub fn random_u8() -> u8 {
        (unsafe { ffi::sys_rand32_get() } & 0xFF) as u8
    }

    pub fn random_u16() -> u16 {
        (unsafe { ffi::sys_rand32_get() } & 0xFFFF) as u16
    }

    pub fn random_u32() -> u32 {
        unsafe { ffi::sys_rand32_get() }
    }

    pub fn random_u64() -> u64 {
        let hi = unsafe { ffi::sys_rand32_get() } as u64;
        let lo = unsafe { ffi::sys_rand32_get() } as u64;
        (hi << 32) | lo
    }

    pub fn random_fill(buf: *mut c_void, len: usize) {
        // sys_rand_get is a static inline in Zephyr headers; go through the
        // real-symbol wrapper.
        unsafe { ffi::nros_zephyr_rand_fill(buf, len) }
    }
}

// ============================================================================
// Time (wall clock) — no RTC, fall back to monotonic
// ============================================================================

impl ZephyrPlatform {
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
// Threading — pthread_* (Zephyr POSIX subsystem)
//
// zenoh-pico's zephyr.h defines:
//   typedef pthread_t _z_task_t;
//   typedef pthread_mutex_t _z_mutex_t;
//   typedef pthread_mutex_t _z_mutex_rec_t;
//   typedef pthread_cond_t _z_condvar_t;
//
// We only ever receive pointers to this storage from zenoh-pico, so the
// exact sizeof doesn't matter to us — pthread_* functions operate on the
// pointed-to memory.
// ============================================================================

/// Adapter: zenoh-pico passes entry as `unsafe extern "C" fn`, pthread_create
/// expects `extern "C" fn`. ABIs are identical on every platform Zephyr runs
/// on; transmute is safe.
type PthreadStart = extern "C" fn(*mut c_void) -> *mut c_void;

impl ZephyrPlatform {
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
        // SAFETY: `unsafe extern "C" fn` and `extern "C" fn` share an ABI.
        let start: PthreadStart = unsafe { core::mem::transmute(entry) };

        // Attr handling: zenoh-pico's zephyr/system.c picks a stack from a
        // static `K_THREAD_STACK_ARRAY` of CONFIG_MAIN_STACK_SIZE slots and
        // calls `pthread_attr_setstack`. We can't replicate that from Rust
        // without allocating stacks, so we rely on Zephyr POSIX's default
        // stack (CONFIG_POSIX_THREAD_DEFAULT_STACK_SIZE). Projects that need
        // custom stacks must override via prj.conf.
        let ret = unsafe { ffi::pthread_create(task, ptr::null(), start, arg) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_join(task: *mut c_void) -> i8 {
        // pthread_t is uint32_t on Zephyr POSIX. We read the value from the
        // storage and pass by value. Using u32 matches the Zephyr ABI.
        let id = unsafe { core::ptr::read_unaligned(task as *const u32) };
        let ret = unsafe { ffi::pthread_join(id, ptr::null_mut()) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_detach(task: *mut c_void) -> i8 {
        let id = unsafe { core::ptr::read_unaligned(task as *const u32) };
        let ret = unsafe { ffi::pthread_detach(id) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_cancel(task: *mut c_void) -> i8 {
        let id = unsafe { core::ptr::read_unaligned(task as *const u32) };
        let ret = unsafe { ffi::pthread_cancel(id) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn task_exit() {
        unsafe { ffi::pthread_exit(ptr::null_mut()) };
    }

    pub fn task_free(task: *mut *mut c_void) {
        // Matches zenoh-pico's zephyr/system.c: free the task slot, null the
        // caller's pointer. pthread_t itself is not a heap object.
        unsafe {
            let t = *task;
            if !t.is_null() {
                Self::dealloc(t);
                *task = ptr::null_mut();
            }
        }
    }

    // -- Mutex --

    pub fn mutex_init(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_mutex_init(m, ptr::null()) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_drop(m: *mut c_void) -> i8 {
        if m.is_null() {
            return 0;
        }
        let ret = unsafe { ffi::pthread_mutex_destroy(m) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_lock(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_mutex_lock(m) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_mutex_trylock(m) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_mutex_unlock(m) };
        if ret == 0 { 0 } else { -1 }
    }

    // -- Recursive mutex --

    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        // pthread_mutexattr_t is opaque; allocate on the stack as a byte
        // buffer large enough for any platform's type (32 bytes is safe —
        // glibc's is 4, Zephyr POSIX is 4, musl is 4).
        let mut attr_buf: [u8; 32] = [0; 32];
        let attr = attr_buf.as_mut_ptr() as *mut c_void;
        unsafe {
            if ffi::pthread_mutexattr_init(attr) != 0 {
                return -1;
            }
            if ffi::pthread_mutexattr_settype(attr, ffi::PTHREAD_MUTEX_RECURSIVE) != 0 {
                ffi::pthread_mutexattr_destroy(attr);
                return -1;
            }
            let ret = ffi::pthread_mutex_init(m, attr);
            ffi::pthread_mutexattr_destroy(attr);
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
        // zenoh-pico uses CLOCK_MONOTONIC for condvar timedwait. Zephyr POSIX
        // supports pthread_condattr_setclock.
        let mut attr_buf: [u8; 32] = [0; 32];
        let attr = attr_buf.as_mut_ptr() as *mut c_void;
        unsafe {
            if ffi::pthread_condattr_init(attr) != 0 {
                return -1;
            }
            // CLOCK_MONOTONIC == 1 on Zephyr POSIX (and Linux).
            ffi::pthread_condattr_setclock(attr, ffi::CLOCK_MONOTONIC);
            let ret = ffi::pthread_cond_init(cv, attr);
            ffi::pthread_condattr_destroy(attr);
            if ret == 0 { 0 } else { -1 }
        }
    }

    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_cond_destroy(cv) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_cond_signal(cv) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_cond_broadcast(cv) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        let ret = unsafe { ffi::pthread_cond_wait(cv, m) };
        if ret == 0 { 0 } else { -1 }
    }

    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime_ms: u64) -> i8 {
        // The shim converts zenoh-pico's timespec to u64 by reading the first
        // 8 bytes (tv_sec). That precision loss is a known limitation — see
        // the crate-level doc comment. Here we reconstruct a timespec assuming
        // the value represents milliseconds since boot (matches how
        // nros-platform-freertos/posix interpret it).
        let ts = ffi::timespec {
            tv_sec: (abstime_ms / 1000) as i64,
            tv_nsec: ((abstime_ms % 1000) * 1_000_000) as i64,
        };
        // pthread_cond_timedwait returns ETIMEDOUT on timeout — map to -1 to
        // match zenoh-pico's Z_ETIMEDOUT convention, same as every other error.
        let ret = unsafe { ffi::pthread_cond_timedwait(cv, m, &ts) };
        if ret == 0 { 0 } else { -1 }
    }
}

// Suppress unused-type warnings for symbols pulled only into specific
// backends (keeps clippy quiet on the posix host test build).
#[allow(dead_code)]
const _: [c_int; 0] = [];
#[allow(dead_code)]
const _: [c_char; 0] = [];
