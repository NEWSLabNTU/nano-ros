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
//! And the readiness notifier's refusal (issue 2144): a poll-only backend
//! learns of an arrival only inside a spin, so it is not offered a notifier
//! that would never fire for a host waiting on it.
//!
//! Run: `just check required-features-tests` (needs the XRCE Agent).

#![cfg(feature = "xrce-wake-test")]

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use nros_node::executor::*;
use nros_tests::fixtures::{XrceAgent, require_xrce_agent};

/// The executor's slice (`POLL_ONLY_WAKE_SLICE_MS`) plus scheduler slack.
const DISPATCH_BOUND: Duration = Duration::from_millis(10 + 40);
/// Far beyond the bound, so a trigger that waits the spin out is unmistakable.
const SPIN_BUDGET: Duration = Duration::from_millis(1000);
const TRIGGER_DELAY: Duration = Duration::from_millis(25);

#[test]
fn guard_trigger_wakes_a_poll_only_executor_within_a_slice() {
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
         executor waited out its {SPIN_BUDGET:?} budget inside the backend's recv"
    );
    println!("guard on XRCE dispatched {latency:?} after the trigger");
}

unsafe extern "C" fn never_called(_ctx: *mut core::ffi::c_void) {
    panic!("a refused readiness notifier was called");
}

/// Issue 2144 -- a poll-only executor REFUSES a readiness notifier instead of
/// installing one that silently never fires; clearing still succeeds.
#[test]
fn a_poll_only_executor_refuses_a_ready_notifier() {
    require_xrce_agent();
    let agent = XrceAgent::start_unique().expect("start XRCE agent");

    let _ = nros_rmw_xrce_cffi::register();
    let addr = agent.addr();
    let config = ExecutorConfig::new(&addr)
        .node_name("xrce_ready_notifier")
        .domain_id(97);
    let mut executor = Executor::open(&config).expect("Executor::open over XRCE");

    assert!(
        !executor.ready_notifier_supported(),
        "XRCE installs no wake callback, so it cannot tell a host anything outside a spin"
    );
    // SAFETY: refused, so never installed; clearing takes no context.
    let set = unsafe { executor.set_ready_notifier(Some(never_called), core::ptr::null_mut()) };
    assert!(
        matches!(
            set,
            Err(NodeError::Transport(nros_node::TransportError::Unsupported))
        ),
        "a poll-only executor accepted a readiness notifier: {set:?}"
    );
    assert!(unsafe { executor.set_ready_notifier(None, core::ptr::null_mut()) }.is_ok());
    // A guard trigger reaches the wake context, where an installed notifier
    // would run: the refused one must not.
    let (_id, guard) = executor
        .register_guard_condition(|| {})
        .expect("register_guard_condition");
    guard.trigger();
    executor.spin_once(Duration::from_millis(10));
}
