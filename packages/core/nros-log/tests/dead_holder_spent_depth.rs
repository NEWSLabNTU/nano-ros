//! Issues 2738 and 2762 -- a holder that dies delivering a HELD record to a
//! re-entrant sink had spent the whole re-entrant depth on it, and releasing
//! the dead holder gives that depth back with the guard.
//!
//! A held record that never reached the re-entrant sinks (the depth was spent
//! when it was raised) is delivered to them by the drain with the depth taken
//! in full, so a re-entrant sink that logs from inside it is bounded by the
//! drain passes. If the holder dies there, the guard is released by the fault
//! handler; without the depth coming back too, every later record would take
//! the guarded path for good, and a record raised while some live thread holds
//! the guard would wait for that thread instead of reaching the re-entrant
//! sink at once.
//!
//! Its own test binary: the dispatch guard, the depth count and the hold queue
//! are process-wide.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nros_log::{LogSink, Logger, Record, Severity, init};

static LOGGER: Logger = Logger::with_level("spent_depth", Severity::Trace);

static PARKED: AtomicUsize = AtomicUsize::new(0);
static RELEASE_PARKED: AtomicBool = AtomicBool::new(false);
static DIED: AtomicBool = AtomicBool::new(false);
static HOLDING: AtomicBool = AtomicBool::new(false);
static RELEASE_HOLD: AtomicBool = AtomicBool::new(false);

fn wait_until(what: &str, cond: impl Fn() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out: {what}"
        );
        std::thread::yield_now();
    }
}

/// Re-entrant. `park` waits inside it (one unit of depth each), `die` never
/// returns (the holder "dies" here), everything else is recorded.
struct Reentrant {
    lines: Mutex<Vec<String>>,
}

impl LogSink for Reentrant {
    fn log(&self, record: &Record<'_>) {
        match record.message {
            "park" => {
                PARKED.fetch_add(1, Ordering::SeqCst);
                wait_until("the parked loggers' release", || {
                    RELEASE_PARKED.load(Ordering::SeqCst)
                });
            }
            "die" => {
                DIED.store(true, Ordering::SeqCst);
                loop {
                    std::thread::park();
                }
            }
            other => self
                .lines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(other.to_string()),
        }
    }

    fn reentrant(&self) -> bool {
        true
    }
}

/// Serialised. `hold` keeps the guard until the test lets it go.
struct Serial;

impl LogSink for Serial {
    fn log(&self, record: &Record<'_>) {
        if record.message == "hold" {
            HOLDING.store(true, Ordering::SeqCst);
            wait_until("the holder's release", || {
                RELEASE_HOLD.load(Ordering::SeqCst)
            });
            HOLDING.store(false, Ordering::SeqCst);
        }
    }
}

static REENTRANT_SINK: Reentrant = Reentrant {
    lines: Mutex::new(Vec::new()),
};
static SERIAL: Serial = Serial;
static SINKS: [&dyn LogSink; 2] = [&REENTRANT_SINK, &SERIAL];

fn reentrant_got(message: &str) -> bool {
    REENTRANT_SINK
        .lines
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|l| l == message)
}

/// Hold the guard from a live thread, inside the serialised sink.
fn hold_guard() -> std::thread::JoinHandle<()> {
    RELEASE_HOLD.store(false, Ordering::SeqCst);
    let t = std::thread::spawn(|| nros_log::log_info!(&LOGGER, "hold"));
    wait_until("a live thread holding the guard", || {
        HOLDING.load(Ordering::SeqCst)
    });
    t
}

#[test]
fn releasing_a_holder_that_died_in_a_held_reentrant_delivery_returns_the_depth() {
    init(&SINKS);
    let depth = nros_log::REENTRANT_DEPTH;

    // Spend the depth: `depth` loggers wait inside the re-entrant sink.
    let parked: Vec<_> = (0..depth)
        .map(|_| std::thread::spawn(|| nros_log::log_info!(&LOGGER, "park")))
        .collect();
    wait_until("every parked logger inside the re-entrant sink", || {
        PARKED.load(Ordering::SeqCst) == depth
    });

    // A live holder, and a record raised now owes EVERY sink: it is held.
    let holder = hold_guard();
    nros_log::log_info!(&LOGGER, "die");

    // The holder drains it, spending the whole depth on its re-entrant
    // delivery, and "dies" inside the re-entrant sink, guard held.
    RELEASE_HOLD.store(true, Ordering::SeqCst);
    wait_until("the holder dying in the re-entrant sink", || {
        DIED.load(Ordering::SeqCst)
    });
    drop(holder); // never joins: it is dead

    RELEASE_PARKED.store(true, Ordering::SeqCst);
    for t in parked {
        t.join().expect("a parked logger panicked");
    }

    // The fault handler's release.
    let frame = nros_log::dispatch_holder_frame();
    assert_ne!(frame, 0, "the dead holder must still hold the guard");
    assert!(nros_log::release_dead_dispatch_holder(frame));

    // With the depth given back, a record raised while a LIVE thread holds
    // the guard reaches the re-entrant sink at once, in this thread. With the
    // depth still spent it would be held for that thread instead.
    let live = hold_guard();
    nros_log::log_info!(&LOGGER, "probe");
    let direct = reentrant_got("probe");
    RELEASE_HOLD.store(true, Ordering::SeqCst);
    live.join().expect("the live holder panicked");
    assert!(
        direct,
        "a record raised under a live holder did not reach the re-entrant sink \
         directly: the dead holder's spent depth was never given back"
    );
}
