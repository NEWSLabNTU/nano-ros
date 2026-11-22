//! Issue 2762 -- concurrent loggers do not queue for a re-entrant sink.
//!
//! Many threads log at once into one re-entrant sink and one that is not.
//! The re-entrant sink receives EVERY record, with several threads inside it
//! at once; the serialised one is still entered one record at a time, and it
//! receives every record or counts it lost, as before.
//!
//! Its own test binary: the dispatch guard and the hold queue are
//! process-wide.

use std::sync::atomic::{AtomicUsize, Ordering};

use nros_log::{LogSink, Logger, Record, Severity, init};

static LOGGER: Logger = Logger::with_level("reentrant", Severity::Trace);

static REENTRANT_GOT: AtomicUsize = AtomicUsize::new(0);
static REENTRANT_INSIDE: AtomicUsize = AtomicUsize::new(0);
static REENTRANT_PEAK: AtomicUsize = AtomicUsize::new(0);
static SERIAL_GOT: AtomicUsize = AtomicUsize::new(0);
static SERIAL_INSIDE: AtomicUsize = AtomicUsize::new(0);

fn widen() {
    for _ in 0..200 {
        std::hint::spin_loop();
    }
}

struct Reentrant;

impl LogSink for Reentrant {
    fn log(&self, record: &Record<'_>) {
        let inside = REENTRANT_INSIDE.fetch_add(1, Ordering::SeqCst) + 1;
        REENTRANT_PEAK.fetch_max(inside, Ordering::SeqCst);
        if record.logger_name == "reentrant" {
            REENTRANT_GOT.fetch_add(1, Ordering::SeqCst);
        }
        widen();
        REENTRANT_INSIDE.fetch_sub(1, Ordering::SeqCst);
    }

    fn reentrant(&self) -> bool {
        true
    }
}

struct Serial;

impl LogSink for Serial {
    fn log(&self, record: &Record<'_>) {
        // The negative control: a sink that is not re-entrant is still never
        // entered twice at once.
        assert_eq!(
            SERIAL_INSIDE.fetch_add(1, Ordering::SeqCst),
            0,
            "two threads in a sink that did not declare itself re-entrant"
        );
        if record.logger_name == "reentrant" {
            SERIAL_GOT.fetch_add(1, Ordering::SeqCst);
        }
        widen();
        SERIAL_INSIDE.fetch_sub(1, Ordering::SeqCst);
    }
}

static REENTRANT_SINK: Reentrant = Reentrant;
static SERIAL: Serial = Serial;
static SINKS: [&dyn LogSink; 2] = [&REENTRANT_SINK, &SERIAL];

// Fewer threads than `REENTRANT_DEPTH`, so no record falls back to the
// guarded path for the re-entrant sink, and far more than the hold queue.
const THREADS: usize = 6;
const PER_THREAD: usize = 2_000;

#[test]
fn a_reentrant_sink_gets_every_record_while_a_serial_one_stays_serial() {
    const { assert!(THREADS < nros_log::REENTRANT_DEPTH) };
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
    nros_log::flush();

    let total = THREADS * PER_THREAD;
    assert_eq!(
        REENTRANT_GOT.load(Ordering::SeqCst),
        total,
        "the re-entrant sink lost records"
    );
    assert!(
        REENTRANT_PEAK.load(Ordering::SeqCst) > 1,
        "no two loggers were ever inside the re-entrant sink at once, so this \
         run proved nothing about concurrency"
    );
    let serial = SERIAL_GOT.load(Ordering::SeqCst);
    let lost = nros_log::early::overflowed();
    assert_eq!(
        serial + lost,
        total,
        "the serialised sink: delivered {serial} + lost {lost} must account for every record"
    );
}
