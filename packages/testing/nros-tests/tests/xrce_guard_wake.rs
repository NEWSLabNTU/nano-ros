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
//! below is set under what that slice could deliver. phase-521 W6 deleted the
//! slice and the receive-blocking wait it cut up, so the bound now guards
//! against either coming back.
//!
//! Run: `just check required-features-tests` (needs the XRCE Agent).

#![cfg(feature = "xrce-wake-test")]

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nros_node::executor::*;
use nros_tests::fixtures::{XrceAgent, require_xrce_agent};

// Issue 3026 -- the claim, and how it is measured so host load cannot fake it.
//
// The claim: the executor parks on the session's socket plus its own wake
// object, so a trigger ends the park at once. The two waits that claim rules
// out both put the trigger behind a RECEIVE timeout rather than behind the
// OS's wake path:
//
// * a whole receive-blocking wait dispatches at the end of the spin's budget,
//   ~975 ms after a trigger 25 ms into a 1000 ms spin;
// * the retired 10 ms guard slice dispatches at the next slice boundary,
//   uniform over 0..10 ms, ~5 ms at the median.
//
// The bound used to be an absolute 3 ms on one sample, and a loaded runner
// put a correct executor at 4.5 ms (issue 3026), because the OS's own wake
// latency is what load inflates. So each trial is paired with a CONTROL: a
// thread parked on a futex, woken by a thread that slept the same delay --
// the same OS wake path the correct executor takes, measured under the same
// load at the same moment. Then:
//
// * the whole-recv wait is ruled out by a SEPARATION, every trial: a quarter
//   of the budget is two orders past any wake and two orders short of it;
// * the slice is ruled out by a RATIO of medians: the executor's wake may
//   cost a few control wakes plus a fixed allowance for its own dispatch,
//   which under no load is ~1 ms against the slice's ~5 ms, and under load
//   grows WITH the control rather than failing a correct executor.

/// Far beyond any wake, so a trigger that waits the spin out is unmistakable.
const SPIN_BUDGET: Duration = Duration::from_millis(1000);
const TRIGGER_DELAY: Duration = Duration::from_millis(25);
/// Odd, so the median is a sample. Each trial costs ~2 x `TRIGGER_DELAY`.
const TRIALS: usize = 9;
/// A trigger dispatched later than this waited inside a receive, not on a
/// wake: ~975 ms for the whole-recv wait, against microseconds for a wake.
const RECV_SEPARATION: Duration = Duration::from_millis(250);
/// The executor's median wake may cost this many control wakes ...
const CONTROL_RATIO: u32 = 4;
/// ... plus this, for the dispatch a futex wake does not do (ready-set scan,
/// guard callback). Measured ~90 us on an idle host, where the retired slice
/// gave ~5 ms.
const DISPATCH_ALLOWANCE: Duration = Duration::from_millis(1);

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

/// The control: this thread parks, another sleeps `TRIGGER_DELAY` and wakes
/// it. Returns wake-to-running latency, the OS's own cost under current load.
fn control_wake() -> Duration {
    let woken = Arc::new(AtomicBool::new(false));
    let me = std::thread::current();
    let flag = woken.clone();
    let waker = std::thread::spawn(move || {
        std::thread::sleep(TRIGGER_DELAY);
        let at = Instant::now();
        flag.store(true, Ordering::SeqCst);
        me.unpark();
        at
    });
    while !woken.load(Ordering::SeqCst) {
        std::thread::park_timeout(SPIN_BUDGET);
    }
    let done = Instant::now();
    let at = waker.join().expect("control waker");
    done.saturating_duration_since(at)
}

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
    let (_id, guard) = executor
        .register_guard_condition(|| FIRED.store(true, Ordering::SeqCst))
        .expect("register_guard_condition");
    let guard = Arc::new(guard);

    // Let the session settle so the measured spins are not its first.
    for _ in 0..5 {
        executor.spin_once(Duration::from_millis(10));
    }

    let mut wakes = Vec::with_capacity(TRIALS);
    let mut controls = Vec::with_capacity(TRIALS);
    for trial in 0..TRIALS {
        // Interleaved, so the control sees the load the trial saw.
        controls.push(control_wake());

        FIRED.store(false, Ordering::SeqCst);
        let g = guard.clone();
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(TRIGGER_DELAY);
            let at = Instant::now();
            g.trigger();
            at
        });
        let start = Instant::now();
        executor.spin_once(SPIN_BUDGET);
        let done = Instant::now();
        let triggered_at = trigger.join().expect("trigger thread");

        assert!(
            FIRED.load(Ordering::SeqCst),
            "trial {trial}: the guard callback did not run in the spin it was triggered in"
        );
        // Lower bound: the spin really waited for the trigger rather than
        // returning at once, which would pass every upper bound vacuously.
        assert!(
            done - start >= TRIGGER_DELAY,
            "trial {trial}: spin_once returned after {:?}, before the trigger at \
             {TRIGGER_DELAY:?}",
            done - start
        );
        let latency = done.saturating_duration_since(triggered_at);
        assert!(
            latency < RECV_SEPARATION,
            "trial {trial}: trigger to dispatch took {latency:?} -- the executor waited \
             inside the backend's recv until its {SPIN_BUDGET:?} budget, instead of \
             parking on the session's socket"
        );
        wakes.push(latency);
    }

    let wake = median(wakes.clone());
    let control = median(controls.clone());
    let allowed = control * CONTROL_RATIO + DISPATCH_ALLOWANCE;
    println!(
        "guard on XRCE: median wake {wake:?} against a median control wake {control:?} \
         (allowed {allowed:?}); trials {wakes:?}, controls {controls:?}"
    );
    assert!(
        wake <= allowed,
        "the median trigger-to-dispatch was {wake:?}, against a median OS wake of {control:?} \
         measured beside it (allowed {CONTROL_RATIO} x that + {DISPATCH_ALLOWANCE:?} = \
         {allowed:?}) -- the trigger waited for a receive slice to end instead of ending \
         the park. Trials: {wakes:?}; controls: {controls:?}"
    );
}
