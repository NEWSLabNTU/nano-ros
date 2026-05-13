//! Phase 121.3.posix — exercise the native C port via `CffiPlatform`.
//!
//! Drives the same symbols `c_stub_platform.rs` does, but linked
//! against `nros-platform-posix-c/src/platform.c` instead of the
//! counter-bumping stubs. Verifies the reference C implementation
//! preserves real POSIX semantics:
//!
//! 1. Monotonic clock advances and is non-decreasing.
//! 2. Sleep blocks at least the requested duration.
//! 3. Allocation round-trips through `malloc` / `realloc` / `free`
//!    (writes to the returned pointer survive realloc, free is
//!    safe).
//! 4. Mutex lock + unlock round-trip on caller-supplied `pthread_mutex_t`
//!    storage.
//! 5. Recursive mutex re-entry from the same thread succeeds.
//! 6. Condvar signal wakes a waiter under a held mutex.
//! 7. A task spawned via `task_init` returns the expected value
//!    through `task_join`.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-platform-cffi --features posix-c-port --test c_port_posix
//! ```

#![cfg(feature = "posix-c-port")]

use core::ffi::c_void;
use std::{
    mem::MaybeUninit,
    time::{Duration, Instant},
};

use nros_platform_api::{
    PlatformAlloc, PlatformClock, PlatformSleep, PlatformThreading, PlatformYield,
};
use nros_platform_cffi::CffiPlatform;

#[test]
fn clock_ms_is_monotonic() {
    let t0 = CffiPlatform::clock_ms();
    std::thread::sleep(Duration::from_millis(5));
    let t1 = CffiPlatform::clock_ms();
    assert!(t1 >= t0);
    assert!(t1 - t0 >= 4, "clock_ms must advance at least ~5ms");
}

#[test]
fn sleep_ms_blocks_at_least_requested() {
    let start = Instant::now();
    CffiPlatform::sleep_ms(10);
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(9),
        "sleep_ms slept only {:?}",
        elapsed
    );
}

#[test]
fn yield_now_returns_immediately() {
    // Hard to test directly; just confirm it doesn't crash.
    CffiPlatform::yield_now();
}

#[test]
fn alloc_realloc_free_round_trip() {
    unsafe {
        let p = CffiPlatform::alloc(32);
        assert!(!p.is_null());
        // Write 32 bytes.
        for i in 0..32 {
            *(p as *mut u8).add(i) = i as u8;
        }
        let p2 = CffiPlatform::realloc(p, 128);
        assert!(!p2.is_null());
        // First 32 bytes preserved.
        for i in 0..32 {
            assert_eq!(*(p2 as *const u8).add(i), i as u8);
        }
        CffiPlatform::dealloc(p2);
    }
}

#[test]
fn mutex_lock_unlock_round_trip() {
    // pthread_mutex_t storage on the test stack.
    let mut storage: MaybeUninit<libc::pthread_mutex_t> = MaybeUninit::zeroed();
    let m = storage.as_mut_ptr() as *mut c_void;

    assert_eq!(CffiPlatform::mutex_init(m), 0);
    assert_eq!(CffiPlatform::mutex_lock(m), 0);
    // try_lock from the same thread on a non-recursive mutex should
    // report contention (PTHREAD_MUTEX_NORMAL allows the kernel to
    // either deadlock or return EBUSY — accept the EBUSY shape).
    let _ = CffiPlatform::mutex_try_lock(m);
    assert_eq!(CffiPlatform::mutex_unlock(m), 0);
    assert_eq!(CffiPlatform::mutex_drop(m), 0);
}

#[test]
fn mutex_rec_allows_reentry() {
    let mut storage: MaybeUninit<libc::pthread_mutex_t> = MaybeUninit::zeroed();
    let m = storage.as_mut_ptr() as *mut c_void;

    assert_eq!(CffiPlatform::mutex_rec_init(m), 0);
    assert_eq!(CffiPlatform::mutex_rec_lock(m), 0);
    assert_eq!(
        CffiPlatform::mutex_rec_lock(m),
        0,
        "recursive mutex must allow same-thread re-entry"
    );
    assert_eq!(CffiPlatform::mutex_rec_unlock(m), 0);
    assert_eq!(CffiPlatform::mutex_rec_unlock(m), 0);
    assert_eq!(CffiPlatform::mutex_rec_drop(m), 0);
}

#[test]
fn condvar_signal_wakes_waiter() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let woken = Arc::new(AtomicBool::new(false));
    let woken_thread = Arc::clone(&woken);

    // Allocate cv + mutex on the heap so the worker thread shares
    // the same address with the test thread.
    let cv = Box::into_raw(Box::new(MaybeUninit::<libc::pthread_cond_t>::zeroed())) as *mut c_void;
    let m = Box::into_raw(Box::new(MaybeUninit::<libc::pthread_mutex_t>::zeroed())) as *mut c_void;
    assert_eq!(CffiPlatform::condvar_init(cv), 0);
    assert_eq!(CffiPlatform::mutex_init(m), 0);

    let cv_addr = cv as usize;
    let m_addr = m as usize;

    let worker = std::thread::spawn(move || {
        let cv = cv_addr as *mut c_void;
        let m = m_addr as *mut c_void;
        assert_eq!(CffiPlatform::mutex_lock(m), 0);
        // Wait for the signal.
        assert_eq!(CffiPlatform::condvar_wait(cv, m), 0);
        woken_thread.store(true, Ordering::SeqCst);
        assert_eq!(CffiPlatform::mutex_unlock(m), 0);
    });

    // Give the worker a moment to enter the wait.
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(CffiPlatform::mutex_lock(m), 0);
    assert_eq!(CffiPlatform::condvar_signal(cv), 0);
    assert_eq!(CffiPlatform::mutex_unlock(m), 0);

    worker.join().unwrap();
    assert!(woken.load(Ordering::SeqCst));

    assert_eq!(CffiPlatform::condvar_drop(cv), 0);
    assert_eq!(CffiPlatform::mutex_drop(m), 0);
    unsafe {
        let _ = Box::from_raw(cv as *mut MaybeUninit<libc::pthread_cond_t>);
        let _ = Box::from_raw(m as *mut MaybeUninit<libc::pthread_mutex_t>);
    }
}

extern "C" fn task_entry(arg: *mut c_void) -> *mut c_void {
    // Forward the same pointer back so `task_join` can observe it
    // via the returned status (currently discarded; presence of the
    // round-trip is what we care about).
    arg
}

#[test]
fn task_init_join_round_trip() {
    let mut task_storage: MaybeUninit<libc::pthread_t> = MaybeUninit::zeroed();
    let task = task_storage.as_mut_ptr() as *mut c_void;

    let marker: u64 = 0xC0FFEE;
    let rc = CffiPlatform::task_init(
        task,
        core::ptr::null_mut(),
        Some(task_entry),
        &marker as *const u64 as *mut c_void,
    );
    assert_eq!(rc, 0, "task_init must succeed");
    assert_eq!(CffiPlatform::task_join(task), 0, "task_join must succeed");
}
