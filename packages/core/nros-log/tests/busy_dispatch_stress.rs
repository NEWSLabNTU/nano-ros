//! Issue 2000 — under contention from many threads, every record is either
//! delivered or counted as lost. None disappears.
//!
//! Its own test binary: the loss counter and the dispatch guard are
//! process-wide.

use std::sync::atomic::{AtomicUsize, Ordering};

use nros_log::{LogSink, Logger, Record, Severity, init};

static LOGGER: Logger = Logger::with_level("stress", Severity::Trace);
static DELIVERED: AtomicUsize = AtomicUsize::new(0);
static INSIDE: AtomicUsize = AtomicUsize::new(0);

struct Count;

impl LogSink for Count {
    fn log(&self, record: &Record<'_>) {
        // One delivery at a time is part of the contract: a sink never sees
        // interleaved calls.
        assert_eq!(
            INSIDE.fetch_add(1, Ordering::SeqCst),
            0,
            "two threads in a sink"
        );
        if record.logger_name == "stress" {
            DELIVERED.fetch_add(1, Ordering::SeqCst);
        }
        // Widen the window another thread can arrive in.
        for _ in 0..50 {
            std::hint::spin_loop();
        }
        INSIDE.fetch_sub(1, Ordering::SeqCst);
    }
}

static COUNT: Count = Count;
static SINKS: [&dyn LogSink; 1] = [&COUNT];

const THREADS: usize = 8;
const PER_THREAD: usize = 5_000;

#[test]
fn every_record_is_delivered_or_counted_as_lost() {
    init(&SINKS);

    let threads: Vec<_> = (0..THREADS)
        .map(|_| {
            std::thread::spawn(|| {
                for i in 0..PER_THREAD {
                    nros_log::log_info!(&LOGGER, "{}", i);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("a logging thread panicked");
    }
    // A record raised during a holder's last pass waits for the next dispatch
    // or flush (`nros_log::early` module docs).
    nros_log::flush();

    let delivered = DELIVERED.load(Ordering::SeqCst);
    let lost = nros_log::early::overflowed();
    assert_eq!(
        delivered + lost,
        THREADS * PER_THREAD,
        "delivered {delivered} + lost {lost} must account for every record"
    );
}
