//! Issue 1906 — zenoh-pico's `_z_condvar_wait_until`, as our alias shim
//! (`zpico-sys/c/zpico/platform_aliases.c`) implements it on POSIX, returns
//! zenoh-pico's own result codes.
//!
//! The platform returns `1` on timeout. The shim passed that through, and
//! zenoh-pico's contract is `Z_ETIMEDOUT` (-71):
//! `_z_sync_group_wait_deadline` tests `ret == Z_ETIMEDOUT`, misses `1`, takes
//! its `ret != _Z_RES_OK` branch and returns while still holding its mutex.
//! This calls the shim the way zenoh-pico does and checks the codes.
//!
//! Needs no router: it exercises the shim and the POSIX port only.

#![cfg(feature = "platform-posix")]

use core::ffi::c_void;
use std::time::{Duration, Instant};

// Keeps the zenoh-pico archive, and with it the alias shim, on the link line.
use nros_rmw_zenoh as _;

/// zenoh-pico's `Z_ETIMEDOUT` (`zenoh-pico/utils/result.h`).
const Z_ETIMEDOUT: i8 = -71;
/// zenoh-pico's `_Z_ERR_GENERIC`.
const Z_ERR_GENERIC: i8 = -128;

unsafe extern "C" {
    fn _z_mutex_init(m: *mut c_void) -> i8;
    fn _z_mutex_drop(m: *mut c_void) -> i8;
    fn _z_mutex_lock(m: *mut c_void) -> i8;
    fn _z_mutex_unlock(m: *mut c_void) -> i8;
    fn _z_condvar_init(cv: *mut c_void) -> i8;
    fn _z_condvar_drop(cv: *mut c_void) -> i8;
    fn _z_condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;
    fn _z_condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime_ms: *const u64) -> i8;
    fn nros_platform_clock_ns() -> u64;
}

/// Storage at least as large and as aligned as the platform's condvar and
/// mutex (`NROS_PLATFORM_{CONDVAR,MUTEX}_STORAGE_SIZE` are 256 and 64).
#[repr(C, align(16))]
struct Storage([u8; 256]);

#[test]
fn a_timed_out_wait_returns_z_etimedout_not_the_platform_code() {
    const WAIT_MS: u64 = 30;
    let mut cv = Box::new(Storage([0; 256]));
    let mut m = Box::new(Storage([0; 256]));
    let cv = cv.0.as_mut_ptr().cast::<c_void>();
    let m = m.0.as_mut_ptr().cast::<c_void>();
    unsafe {
        assert_eq!(_z_condvar_init(cv), 0);
        assert_eq!(_z_mutex_init(m), 0);

        let deadline = nros_platform_clock_ns() / 1_000_000 + WAIT_MS;
        let start = Instant::now();
        assert_eq!(_z_mutex_lock(m), 0);
        // A spurious wake returns 0 early; wait again for the same deadline.
        let rc = loop {
            let rc = _z_condvar_wait_until(cv, m, &deadline);
            if rc != 0 {
                break rc;
            }
        };
        assert_eq!(_z_mutex_unlock(m), 0);

        assert_eq!(
            rc, Z_ETIMEDOUT,
            "a timed-out wait must return zenoh-pico's Z_ETIMEDOUT (-71), got {rc}"
        );
        assert!(start.elapsed() >= Duration::from_millis(WAIT_MS - 1));

        assert_eq!(_z_condvar_drop(cv), 0);
        assert_eq!(_z_mutex_drop(m), 0);
    }
}

#[test]
fn a_null_deadline_is_an_error_not_an_unbounded_wait() {
    let mut cv = Box::new(Storage([0; 256]));
    let mut m = Box::new(Storage([0; 256]));
    let cv = cv.0.as_mut_ptr().cast::<c_void>();
    let m = m.0.as_mut_ptr().cast::<c_void>();
    unsafe {
        assert_eq!(_z_condvar_init(cv), 0);
        assert_eq!(_z_mutex_init(m), 0);
        assert_eq!(_z_mutex_lock(m), 0);
        // Before issue 1906 this fell back to the unbounded wait and, with no
        // signaller, never returned.
        assert_eq!(
            _z_condvar_wait_until(cv, m, core::ptr::null()),
            Z_ERR_GENERIC
        );
        assert_eq!(_z_mutex_unlock(m), 0);
        assert_eq!(_z_condvar_drop(cv), 0);
        assert_eq!(_z_mutex_drop(m), 0);
    }
}

/// Issue 1196 / phase-436 D2 -- `_z_condvar_wait` is zenoh-pico's UNBOUNDED
/// wait, and the platform ABI no longer has one. The shim bridges it with one
/// bounded wait on a deadline it owns (`NROS_ZP_CONDVAR_SLICE_MS`, 1000 ms) and
/// reports the slice expiring as a spurious wake (`0`), which every caller
/// absorbs by re-testing its predicate. With no signaller this used to block
/// forever; it must now come back, with `0`, after about one slice -- and not
/// early, which would turn every idle zenoh-pico wait into a poll.
#[test]
fn an_unsignalled_wait_returns_as_a_spurious_wake_after_one_slice() {
    let mut cv = Box::new(Storage([0; 256]));
    let mut m = Box::new(Storage([0; 256]));
    let cv = cv.0.as_mut_ptr().cast::<c_void>();
    let m = m.0.as_mut_ptr().cast::<c_void>();
    unsafe {
        assert_eq!(_z_condvar_init(cv), 0);
        assert_eq!(_z_mutex_init(m), 0);
        assert_eq!(_z_mutex_lock(m), 0);
        let start = Instant::now();
        let rc = _z_condvar_wait(cv, m);
        let elapsed = start.elapsed();
        assert_eq!(_z_mutex_unlock(m), 0);
        assert_eq!(rc, 0, "an expired slice is a spurious wake (0), got {rc}");
        assert!(
            elapsed >= Duration::from_millis(990),
            "the slice must not end early (an idle wait would become a poll): {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "the wait must be bounded by its slice: {elapsed:?}"
        );
        assert_eq!(_z_condvar_drop(cv), 0);
        assert_eq!(_z_mutex_drop(m), 0);
    }
}
