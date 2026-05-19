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

// ----------------------------------------------------------------------
// Phase 110.E.b — Rust trait-side coverage.
// ----------------------------------------------------------------------

extern "C" fn bump_safe(user_data: *mut c_void) {
    // Same as `bump` above, expressed as a safe `extern "C" fn` so it
    // satisfies the `PlatformTimer` trait's callback type. The trait
    // takes a non-`unsafe` fn pointer; the C impl signature is
    // `unsafe extern "C" fn`. The cffi shim coerces between them.
    let counter = unsafe { &*(user_data as *const AtomicU32) };
    counter.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn rust_trait_periodic_fires() {
    use nros_platform_api::PlatformTimer;

    let counter = AtomicU32::new(0);
    let handle = CffiPlatform::create_periodic(
        5_000, // 5 ms
        bump_safe,
        &counter as *const _ as *mut c_void,
    )
    .expect("create_periodic via Rust trait");

    thread::sleep(Duration::from_millis(40));
    CffiPlatform::destroy(handle);

    let count = counter.load(Ordering::SeqCst);
    assert!(
        count >= 4,
        "expected at least 4 fires via trait surface, got {count}"
    );
}

#[test]
fn rust_trait_cancel_returns_true_when_prevented() {
    use nros_platform_api::PlatformTimer;

    let counter = AtomicU32::new(0);
    let mut handle = CffiPlatform::create_oneshot(
        100_000, // 100 ms
        bump_safe,
        &counter as *const _ as *mut c_void,
    )
    .expect("create_oneshot via Rust trait");

    thread::sleep(Duration::from_millis(5));
    let prevented = CffiPlatform::cancel(&mut handle);
    assert!(
        prevented,
        "cancel via Rust trait must return true when fire prevented"
    );
    CffiPlatform::destroy(handle);
}

// ----------------------------------------------------------------------
// Phase 110.E.b — End-to-end Sporadic-state refill via the trait.
// ----------------------------------------------------------------------
//
// This is the headline integration: drive
// `AtomicSporadicState::budget_remaining_us` from a real platform
// timer + the shipped `atomic_sporadic_refill_thunk`. Demonstrates
// the wake-up path the Executor will take inside
// `register_sporadic_timer` without pulling the rest of `nros-node`
// into this test crate.

#[test]
fn rust_trait_atomic_sporadic_refill_round_trip() {
    use nros_node::executor::sched_context::{AtomicSporadicState, atomic_sporadic_refill_thunk};
    use nros_platform_api::PlatformTimer;
    use std::sync::Arc;

    let state = Arc::new(AtomicSporadicState::new(10_000, 5_000));

    // Drain the budget so we can prove the refill thunk restored it.
    state.consume(10_000);
    assert!(
        !state.has_budget(),
        "budget should be exhausted before refill"
    );

    let user_data = Arc::as_ptr(&state) as *mut c_void;
    let handle = CffiPlatform::create_periodic(
        2_000, // 2 ms — refill fires several times within the wait window
        atomic_sporadic_refill_thunk,
        user_data,
    )
    .expect("create_periodic via Rust trait");

    thread::sleep(Duration::from_millis(30));
    CffiPlatform::destroy(handle);

    assert!(
        state.has_budget(),
        "atomic sporadic state should be refilled by the timer callback"
    );
    let remaining = state.budget_remaining_us.load(Ordering::Acquire);
    assert_eq!(
        remaining, 10_000,
        "refill thunk must restore budget to its declared capacity"
    );
}
