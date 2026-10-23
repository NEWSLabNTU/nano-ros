//! Issue 2000 — a record raised by another thread while a sink is running is
//! delivered, not dropped.
//!
//! Its own test binary: `init` publishes process-global state, and the
//! dispatch guard is process-wide, so a second test in this binary could hold
//! it at the wrong moment.
//!
//! Before the fix, dispatch held one process-wide flag across every
//! `sink.log()` and DROPPED any record that found it set. That was meant for a
//! sink that logs, but it also dropped every record from another thread.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nros_log::{LogSink, Logger, Record, Severity, init};

static IN_SINK: AtomicBool = AtomicBool::new(false);
static OTHER_DONE: AtomicBool = AtomicBool::new(false);

fn wait_for(flag: &AtomicBool) -> bool {
    let start = Instant::now();
    while !flag.load(Ordering::SeqCst) {
        if start.elapsed() > Duration::from_secs(5) {
            return false;
        }
        std::thread::yield_now();
    }
    true
}

struct Slow {
    lines: Mutex<Vec<String>>,
}

impl LogSink for Slow {
    fn log(&self, record: &Record<'_>) {
        if record.message == "slow" {
            // Stay inside the sink until the other thread's log call has
            // RETURNED: its record must not depend on this sink finishing first.
            IN_SINK.store(true, Ordering::SeqCst);
            assert!(wait_for(&OTHER_DONE), "the other thread never logged");
        }
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(record.message.to_string());
    }
}

static SLOW: Slow = Slow {
    lines: Mutex::new(Vec::new()),
};
static SINKS: [&dyn LogSink; 1] = [&SLOW];
static LOGGER: Logger = Logger::with_level("busy_dispatch", Severity::Trace);

#[test]
fn a_record_from_another_thread_during_delivery_is_delivered() {
    init(&SINKS);

    let other = std::thread::spawn(|| {
        assert!(
            wait_for(&IN_SINK),
            "the first record never reached the sink"
        );
        nros_log::log_info!(&LOGGER, "from the other thread");
        OTHER_DONE.store(true, Ordering::SeqCst);
    });

    nros_log::log_info!(&LOGGER, "slow");
    other.join().expect("the other thread panicked");

    let lines = SLOW.lines.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(
        *lines,
        vec!["slow".to_string(), "from the other thread".to_string()],
        "the other thread's record must arrive, after the one being delivered"
    );
    assert_eq!(nros_log::early::overflowed(), 0);
}
