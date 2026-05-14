//! Phase 124.B.7.d — ISR-safe wake contract test.
//!
//! Verifies that `GuardConditionHandle::trigger()` from a separate
//! thread unblocks a `spin_once` blocked on `wake_cv` within a tight
//! latency bound. This exercises the wake-callback path landed in
//! Phase 124.B (commits 2e5204ca → 2d1009f5).
//!
//! Scope today: thread-context trigger (the realistic ISR-like
//! context — kernel timer callback, worker thread, etc.). POSIX
//! signal-handler trigger pending B.7.c signalfd worker.
//!
//! Run: `cargo test -p nros-tests --test wake_latency --features trigger-test -- --ignored`
//!
//! Both tests are `#[ignore]` by default — in-process `Executor::open`
//! against the `zenohd_unique` fixture fails to connect in the current
//! test harness setup (same failure mode as `trigger_conditions.rs`'s
//! lone test). Pre-existing issue not specific to Phase 124. The
//! tests are written so they exercise the right contract once the
//! in-process zenoh-pico Client-to-zenohd setup is fixed.

#![cfg(feature = "trigger-test")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nros_node::executor::*;
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

/// Wake-latency bound. The user-visible promise of Phase 124.B is
/// "condvar-bound wake instead of poll-period-bound". On POSIX
/// std::Condvar typically wakes within 100 µs; we allow 10 ms to
/// absorb CI scheduler jitter. Pre-124.B would deadline at the
/// `spin_once` timeout (set to 1000 ms below), so any number < 1s
/// is a verifiable improvement.
const WAKE_LATENCY_BOUND_MS: u64 = 10;

/// Cross-thread trigger latency: spawn a worker thread, sleep a
/// known delay, trigger the guard. Main thread is blocked in
/// `spin_once(1000ms)`. Measure how long spin_once actually took
/// versus the trigger fire time — must be ≤ trigger_delay +
/// WAKE_LATENCY_BOUND_MS.
#[rstest]
#[ignore = "in-process Executor::open against zenohd_unique fixture fails (see file header)"]
fn wake_latency_cross_thread_trigger(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();
    let config = ExecutorConfig::new(&locator)
        .node_name("wake_latency_test")
        .domain_id(96);

    let mut executor = Executor::open(&config).expect("Executor::open failed");

    static GUARD_FIRED: AtomicBool = AtomicBool::new(false);
    GUARD_FIRED.store(false, Ordering::SeqCst);

    let (_guard_id, guard_handle) = executor
        .register_guard_condition(|| {
            GUARD_FIRED.store(true, Ordering::SeqCst);
        })
        .expect("register_guard_condition failed");

    // Spawn trigger thread: sleep 50ms, then trigger. The 50ms
    // delay is long enough that the main thread is firmly inside
    // its blocking cv-wait when trigger fires.
    const TRIGGER_DELAY_MS: u64 = 50;
    let trigger_time = std::sync::Arc::new(std::sync::Mutex::new(None::<Instant>));
    let trigger_time_thread = std::sync::Arc::clone(&trigger_time);

    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(TRIGGER_DELAY_MS));
        let fire = Instant::now();
        *trigger_time_thread.lock().unwrap() = Some(fire);
        guard_handle.trigger();
    });

    // Main thread: spin_once with 1000ms timeout. Without 124.B
    // this would deadline at 1000ms (or whichever next-deadline
    // capping shows). Post-124.B, the trigger from the worker
    // thread should unblock us in TRIGGER_DELAY_MS + epsilon.
    let spin_start = Instant::now();
    executor.spin_once(Duration::from_millis(1000));
    let spin_elapsed = spin_start.elapsed();

    handle.join().unwrap();

    let fire = trigger_time
        .lock()
        .unwrap()
        .expect("trigger thread did not record fire time");
    let latency = fire.elapsed();

    assert!(
        GUARD_FIRED.load(Ordering::SeqCst),
        "Guard callback never ran"
    );
    assert!(
        spin_elapsed < Duration::from_millis(TRIGGER_DELAY_MS + WAKE_LATENCY_BOUND_MS + 50),
        "spin_once took {:?} — expected ≤ {}+{}+50 ms (worker delay + wake bound + slack)",
        spin_elapsed,
        TRIGGER_DELAY_MS,
        WAKE_LATENCY_BOUND_MS,
    );
    assert!(
        latency < Duration::from_millis(WAKE_LATENCY_BOUND_MS),
        "Trigger-to-spin-exit latency {:?} exceeds {}ms bound — cv wake not firing",
        latency,
        WAKE_LATENCY_BOUND_MS,
    );

    println!(
        "SUCCESS: spin_once unblocked {} ms after trigger (≤ {} ms bound); total spin {:?}",
        latency.as_millis(),
        WAKE_LATENCY_BOUND_MS,
        spin_elapsed,
    );
}

/// Negative control: WITHOUT the trigger, spin_once must honour
/// its full timeout. Confirms the cv-wait is bounded by the user's
/// timeout (not infinite block).
#[rstest]
#[ignore = "in-process Executor::open against zenohd_unique fixture fails (see file header)"]
fn spin_once_honours_timeout_without_trigger(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let locator = zenohd_unique.locator();
    let config = ExecutorConfig::new(&locator)
        .node_name("wake_timeout_test")
        .domain_id(95);

    let mut executor = Executor::open(&config).expect("Executor::open failed");

    let (_id, _handle) = executor
        .register_guard_condition(|| {})
        .expect("register_guard_condition failed");

    let timeout_ms = 100;
    let start = Instant::now();
    executor.spin_once(Duration::from_millis(timeout_ms));
    let elapsed = start.elapsed();

    let lower = Duration::from_millis(timeout_ms - 10);
    let upper = Duration::from_millis(timeout_ms + 50);
    assert!(
        elapsed >= lower,
        "spin_once returned in {:?} — earlier than {timeout_ms}ms timeout (spurious wake?)",
        elapsed
    );
    assert!(
        elapsed <= upper,
        "spin_once blocked {:?} — much longer than {timeout_ms}ms timeout",
        elapsed
    );

    println!(
        "SUCCESS: spin_once honoured {}ms timeout (took {:?})",
        timeout_ms, elapsed
    );
}
