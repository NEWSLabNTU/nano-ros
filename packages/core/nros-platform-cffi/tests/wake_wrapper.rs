//! Phase 130.2 — exercise the `Wake<CffiPlatform>` ergonomic wrapper
//! against the POSIX C port. Verifies the trait-based RAII path
//! that the executor will use, not just the bare FFI.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-platform-cffi --features posix-c-port --test wake_wrapper
//! ```

#![cfg(feature = "posix-c-port")]

// Force-link the platform-posix staticlib (see c_port_posix_wake.rs).
use nros_platform_cffi as _;

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use nros_platform_api::{Wake, WakeInitError, WakeReason};
use nros_platform_cffi::CffiPlatform;

#[test]
fn new_succeeds_on_posix() {
    let w: Wake<CffiPlatform> = Wake::new().expect("Wake::new failed on POSIX");
    drop(w);
}

#[test]
fn wait_times_out_when_unsigned() {
    let w: Wake<CffiPlatform> = Wake::new().unwrap();

    let t0 = Instant::now();
    let r = w.wait_ms(50);
    let elapsed = t0.elapsed();

    assert_eq!(r, WakeReason::Timeout);
    assert!(elapsed >= Duration::from_millis(45));
    assert!(elapsed < Duration::from_millis(500));
}

#[test]
fn pre_signal_then_wait_returns_immediately() {
    let w: Wake<CffiPlatform> = Wake::new().unwrap();

    w.signal();
    let t0 = Instant::now();
    let r = w.wait_ms(5_000);
    let elapsed = t0.elapsed();

    assert_eq!(r, WakeReason::Signaled);
    assert!(elapsed < Duration::from_millis(100));
}

#[test]
fn cross_thread_signal_wakes_waiter() {
    let w = Arc::new(Wake::<CffiPlatform>::new().unwrap());
    let signaller_handle = {
        let w = Arc::clone(&w);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            w.signal();
        })
    };

    let t0 = Instant::now();
    let r = w.wait_ms(5_000);
    let elapsed = t0.elapsed();
    signaller_handle.join().unwrap();

    assert_eq!(r, WakeReason::Signaled);
    assert!(elapsed >= Duration::from_millis(80));
    assert!(elapsed < Duration::from_millis(1_000));
}

#[test]
fn signal_from_isr_aliases_to_signal_on_posix() {
    let w: Wake<CffiPlatform> = Wake::new().unwrap();

    // POSIX has no real ISR context; the default forward returns
    // success (0). Wrapper reports `true` on success.
    assert!(w.signal_from_isr());
    assert_eq!(w.wait_ms(50), WakeReason::Signaled);
}

#[test]
fn double_signal_coalesces_to_one() {
    let w: Wake<CffiPlatform> = Wake::new().unwrap();

    w.signal();
    w.signal();
    assert_eq!(w.wait_ms(50), WakeReason::Signaled);
    // Second wait must time out — the binary semaphore stayed at 1.
    assert_eq!(w.wait_ms(50), WakeReason::Timeout);
}

#[test]
fn init_error_when_storage_too_small_is_unreachable_on_posix() {
    // sem_t on Linux x86_64 is 32 bytes; macOS path is ~72.
    // WAKE_STORAGE_BYTES=128 covers both. Sanity-check the probe
    // returns a fit-in-buffer size.
    use nros_platform_api::PlatformThreading;
    let needed = CffiPlatform::wake_storage_size();
    assert!(needed > 0, "probe returned 0 — wake unsupported on POSIX?");
    assert!(
        needed <= nros_platform_api::WAKE_STORAGE_BYTES,
        "probe={} > WAKE_STORAGE_BYTES={}",
        needed,
        nros_platform_api::WAKE_STORAGE_BYTES,
    );
}

#[test]
fn explicit_unsupported_path_returns_initerror() {
    // Synthesize an "unsupported" platform by hand to confirm
    // `Wake::new` maps `wake_init -> -1` to `Unsupported`. Uses a
    // local stub platform that overrides only wake_storage_size +
    // wake_init.
    struct StubPlatform;
    impl nros_platform_api::PlatformThreading for StubPlatform {
        fn task_init(
            _: *mut std::ffi::c_void,
            _: *mut std::ffi::c_void,
            _: Option<unsafe extern "C" fn(*mut std::ffi::c_void) -> *mut std::ffi::c_void>,
            _: *mut std::ffi::c_void,
        ) -> i8 {
            -1
        }
        fn task_join(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn task_detach(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn task_cancel(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn task_exit() {}
        fn task_free(_: *mut *mut std::ffi::c_void) {}
        fn mutex_init(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_drop(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_lock(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_try_lock(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_unlock(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_rec_init(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_rec_drop(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_rec_lock(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_rec_try_lock(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn mutex_rec_unlock(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn condvar_init(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn condvar_drop(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn condvar_signal(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn condvar_signal_all(_: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn condvar_wait(_: *mut std::ffi::c_void, _: *mut std::ffi::c_void) -> i8 {
            -1
        }
        fn condvar_wait_until(_: *mut std::ffi::c_void, _: *mut std::ffi::c_void, _: u64) -> i8 {
            -1
        }
        // wake_* all left as defaults — `wake_storage_size` -> 0.
    }

    let r: Result<Wake<StubPlatform>, _> = Wake::new();
    assert_eq!(r.err(), Some(WakeInitError::Unsupported));
}
