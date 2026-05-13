//! Phase 121.6.posix-c — runtime tests against the POSIX C timer port.

#![cfg(feature = "posix-c-port")]

use core::ffi::c_void;
use std::{
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::Duration,
};

#[allow(unused_imports)]
use nros_platform_cffi::CffiPlatform;

unsafe extern "C" {
    fn nros_platform_timer_create_periodic(
        period_us: u32,
        callback: unsafe extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> *mut c_void;
    fn nros_platform_timer_create_oneshot(
        timeout_us: u32,
        callback: unsafe extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> *mut c_void;
    fn nros_platform_timer_destroy(handle: *mut c_void);
    fn nros_platform_timer_cancel(handle: *mut c_void) -> i8;
}

unsafe extern "C" fn bump(user_data: *mut c_void) {
    let counter = unsafe { &*(user_data as *const AtomicU32) };
    counter.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn periodic_timer_fires_repeatedly() {
    let counter = AtomicU32::new(0);
    let handle = unsafe {
        nros_platform_timer_create_periodic(
            5_000, // 5 ms
            bump,
            &counter as *const _ as *mut c_void,
        )
    };
    assert!(!handle.is_null(), "create_periodic must succeed");

    thread::sleep(Duration::from_millis(40));
    unsafe { nros_platform_timer_destroy(handle) };

    let count = counter.load(Ordering::SeqCst);
    assert!(
        count >= 4,
        "expected at least 4 fires over 40 ms, got {count}"
    );
}

#[test]
fn oneshot_timer_fires_once() {
    let counter = AtomicU32::new(0);
    let handle = unsafe {
        nros_platform_timer_create_oneshot(5_000, bump, &counter as *const _ as *mut c_void)
    };
    assert!(!handle.is_null());

    thread::sleep(Duration::from_millis(40));
    let count = counter.load(Ordering::SeqCst);
    assert_eq!(count, 1, "oneshot must fire exactly once");

    unsafe { nros_platform_timer_destroy(handle) };
}

#[test]
fn cancel_prevents_oneshot_fire() {
    let counter = AtomicU32::new(0);
    let handle = unsafe {
        nros_platform_timer_create_oneshot(
            100_000, // 100 ms — comfortable cancellation margin
            bump,
            &counter as *const _ as *mut c_void,
        )
    };
    assert!(!handle.is_null());

    // Cancel almost immediately.
    thread::sleep(Duration::from_millis(5));
    let rc = unsafe { nros_platform_timer_cancel(handle) };
    assert_eq!(rc, 1, "cancel must report prevent-fire");

    thread::sleep(Duration::from_millis(120));
    let count = counter.load(Ordering::SeqCst);
    assert_eq!(count, 0, "callback must not have fired after cancel");

    unsafe { nros_platform_timer_destroy(handle) };
}
