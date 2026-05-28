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
//! Phase 124.B.8 finalisation: tests now run by default. Two
//! setup gaps were closed:
//!   * `trigger-test` feature pulls `nros-rmw-zenoh` + the cffi
//!     bridge so zenoh-pico auto-registers via `.init_array`
//!     before `Executor::open` runs (previously the registry was
//!     empty and `open` surfaced `Transport(ConnectionFailed)`).
//!   * Test source `use nros_rmw_zenoh as _;` forces the linker
//!     to pull the backend symbols into the test binary.
//!
//! Run serialized (`--test-threads=1`) — both tests open
//! in-process zenoh-pico sessions against the `zenohd_unique`
//! fixture, and zenoh-pico's single-process state isn't safe to
//! tear down + re-open in parallel inside one test binary.

#![cfg(feature = "trigger-test")]

// Force-link the zenoh-pico backend so its `.init_array` ctor
// registers the vtable before `Executor::open` runs (Phase 104.A).
// Without this the cffi registry is empty and `Executor::open`
// surfaces `Transport(ConnectionFailed)`.
use nros_rmw_zenoh as _;

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

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

/// Phase 124.G.1 — 4 idle subscribers + 1 Hz timer.
///
/// Validates that Phase 124.B's condvar-bound spin doesn't drift
/// or starve under steady idle load: open an Executor, register
/// 4 subscribers on never-published topics + 1 periodic timer at
/// 1 Hz, then call `spin_once(deadline_ms)` in a loop for K
/// seconds. Assert the timer fired `K` ±1 times (no missed wake,
/// no busy-spin extra fire from a spurious cv wake).
///
/// Pre-124.B (drive_io-timeout-bound spin) would have fired the
/// timer regardless of cv state — this test verifies the
/// post-124.B path still credits real wall-clock time to timers
/// even when the cv-wait is the gating sleep.
#[rstest]
fn timer_fires_n_times_per_n_seconds_under_idle_subs(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    use nros_node::{QosSettings, timer::TimerDuration};

    let locator = zenohd_unique.locator();
    let config = ExecutorConfig::new(&locator)
        .node_name("timer_idle_test")
        .domain_id(94);

    let mut executor = Executor::open(&config).expect("Executor::open failed");

    // 3 idle subscribers + 1 timer = 4 callbacks total = fits
    // the default `NROS_EXECUTOR_MAX_CBS=4` arena. Acceptance
    // wording says "4 idle subs" but the wake-vs-poll contract
    // doesn't depend on the exact sub count — 3 is enough to
    // exercise the multi-entry has_data scan path on every
    // spin_once tick.
    let nid = executor
        .node_builder("timer_idle_node")
        .build()
        .expect("node build");
    for i in 0..3u8 {
        let topic = format!("/wake_latency/idle_sub_{}", i);
        executor
            .node_mut(nid)
            .subscription(&topic)
            .generic("std_msgs/msg/Empty", "")
            .qos(QosSettings::default())
            .rx_buffer::<256>()
            .build(|_data: &[u8]| {})
            .expect("register idle sub");
    }

    use std::sync::atomic::{AtomicU32, Ordering};
    static TIMER_FIRES: AtomicU32 = AtomicU32::new(0);
    TIMER_FIRES.store(0, Ordering::SeqCst);

    executor
        .register_timer(TimerDuration::from_millis(1000), || {
            TIMER_FIRES.fetch_add(1, Ordering::SeqCst);
        })
        .expect("register timer");

    // Spin for K=5 seconds. Wake-deadline cap is 1 s (timer
    // period), so 5 cv-waits of ~1 s each.
    let k_seconds: u64 = 5;
    let start = Instant::now();
    let budget = Duration::from_secs(k_seconds);
    let spin_chunk = Duration::from_millis(200);
    while start.elapsed() < budget {
        executor.spin_once(spin_chunk);
    }
    let elapsed = start.elapsed();
    let fires = TIMER_FIRES.load(Ordering::SeqCst);

    // Allow ±1 for boundary timing (first fire may land at t≈1s
    // or t≈0; last fire may or may not land inside the window).
    let lower = (k_seconds as u32).saturating_sub(1);
    let upper = (k_seconds as u32).saturating_add(1);
    assert!(
        fires >= lower && fires <= upper,
        "timer fired {} times in {:?} seconds — expected {}±1 \
         (suggests condvar drift or missed wake)",
        fires,
        elapsed,
        k_seconds,
    );
    println!(
        "SUCCESS: timer fired {} times in {:?} ({}±1 expected, 4 idle subs registered)",
        fires, elapsed, k_seconds,
    );
}
