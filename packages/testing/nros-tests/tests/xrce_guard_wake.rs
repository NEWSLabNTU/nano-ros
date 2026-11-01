//! A guard condition triggered from another thread wakes an executor whose
//! backend is POLL-ONLY (XRCE) within one transport slice, not at the end of
//! the spin's bound.
//!
//! phase-436 / RFC-0106 D5. A poll-only backend installs no wake callback, so
//! the executor's wait is the backend's own `recv`, which a trigger cannot end:
//! the trigger signals the wake object and nobody waits on it. Before the
//! sliced drive, a trigger 20 ms into a 100 / 500 / 1000 ms spin was dispatched
//! 79 / 480 / 979 ms later. The zenoh sibling of this test is
//! `wake_latency.rs`; that backend wakes the executor itself.
//!
//! phase-521 W4 (RFC-0113 D3/D4) — XRCE over UDP now reports its socket as
//! WAITABLE readiness, and the executor parks on that socket plus its own wake
//! object, so the trigger ends the park directly: measured ~90 us on a Linux
//! host, where the 10 ms slice gave ~5 ms for this trigger's phase. The bound
//! below is set under what the slice can deliver, so a session that falls back
//! to the sliced receive-blocking wait fails it.
//!
//! Run: `just check required-features-tests` (needs the XRCE Agent).

#![cfg(feature = "xrce-wake-test")]

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use nros_node::executor::*;
use nros_tests::fixtures::{XrceAgent, require_xrce_agent};

/// The OS wake latency plus scheduler slack — under the ~5 ms the old 10 ms
/// slice (`POLL_ONLY_WAKE_SLICE_MS`) gives a trigger 25 ms into the spin.
const DISPATCH_BOUND: Duration = Duration::from_millis(3);
/// Far beyond the bound, so a trigger that waits the spin out is unmistakable.
const SPIN_BUDGET: Duration = Duration::from_millis(1000);
const TRIGGER_DELAY: Duration = Duration::from_millis(25);

#[test]
fn guard_trigger_wakes_an_xrce_executor_parked_on_its_socket() {
    require_xrce_agent();
    let agent = XrceAgent::start_unique().expect("start XRCE agent");

    let _ = nros_rmw_xrce_cffi::register();
    let addr = agent.addr();
    let config = ExecutorConfig::new(&addr)
        .node_name("xrce_guard_wake")
        .domain_id(97);
    let mut executor = Executor::open(&config).expect("Executor::open over XRCE");

    static FIRED: AtomicBool = AtomicBool::new(false);
    FIRED.store(false, Ordering::SeqCst);
    let (_id, guard) = executor
        .register_guard_condition(|| FIRED.store(true, Ordering::SeqCst))
        .expect("register_guard_condition");

    // Let the session settle so the measured spin is not its first.
    for _ in 0..5 {
        executor.spin_once(Duration::from_millis(10));
    }

    let trigger = std::thread::spawn(move || {
        std::thread::sleep(TRIGGER_DELAY);
        let at = Instant::now();
        guard.trigger();
        at
    });

    let start = Instant::now();
    executor.spin_once(SPIN_BUDGET);
    let done = Instant::now();
    let triggered_at = trigger.join().expect("trigger thread");

    assert!(
        FIRED.load(Ordering::SeqCst),
        "the guard callback did not run in the spin it was triggered in"
    );
    // Lower bound: the spin really waited for the trigger rather than
    // returning at once, which would pass the upper bound vacuously.
    assert!(
        done - start >= TRIGGER_DELAY,
        "spin_once returned after {:?}, before the trigger at {TRIGGER_DELAY:?}",
        done - start
    );
    let latency = done.saturating_duration_since(triggered_at);
    assert!(
        latency <= DISPATCH_BOUND,
        "trigger to dispatch took {latency:?} (bound {DISPATCH_BOUND:?}) — the \
         executor waited inside the backend's recv (sliced or whole) instead of \
         parking on the session's socket"
    );
    println!("guard on XRCE dispatched {latency:?} after the trigger");
}
