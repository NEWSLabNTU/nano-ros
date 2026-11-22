//! Issue 2762 -- a re-entrant sink that logs from inside its own `log()` is
//! not recursed into without bound.
//!
//! The worst case: a sink that declares itself re-entrant and then logs on
//! every write, its own records included. Each nested record is one more
//! re-entrant delivery in progress; past `REENTRANT_DEPTH` the record takes
//! the guarded path, which delivers a bounded number of further lines
//! (issue 2000's drain passes). The test ends, which is the point.
//!
//! Its own test binary: the dispatch guard and the hold queue are
//! process-wide, and this test leaves records in the queue.

use std::sync::atomic::{AtomicUsize, Ordering};

use nros_log::{LogSink, Logger, Record, Severity, init};

static LOGGER: Logger = Logger::with_level("reentrant_logs", Severity::Trace);
static LINES: AtomicUsize = AtomicUsize::new(0);
static DEPTH: AtomicUsize = AtomicUsize::new(0);
static DEEPEST: AtomicUsize = AtomicUsize::new(0);

struct Echo;

impl LogSink for Echo {
    fn log(&self, _record: &Record<'_>) {
        let depth = DEPTH.fetch_add(1, Ordering::SeqCst) + 1;
        DEEPEST.fetch_max(depth, Ordering::SeqCst);
        LINES.fetch_add(1, Ordering::SeqCst);
        nros_log::log_info!(&LOGGER, "echo");
        DEPTH.fetch_sub(1, Ordering::SeqCst);
    }

    fn reentrant(&self) -> bool {
        true
    }
}

static ECHO: Echo = Echo;
static SINKS: [&dyn LogSink; 1] = [&ECHO];

#[test]
fn a_reentrant_sink_that_logs_is_stopped() {
    init(&SINKS);

    nros_log::log_info!(&LOGGER, "first");

    let deepest = DEEPEST.load(Ordering::SeqCst);
    assert!(
        deepest <= nros_log::REENTRANT_DEPTH + 1,
        "the sink was entered {deepest} deep, past the bound of {}",
        nros_log::REENTRANT_DEPTH
    );
    // The bound: the first record, one line per direct level, and at each of
    // those levels plus the outermost, one line per drain pass (a record the
    // sink raised was held, and each pass delivers what was held when it
    // began).
    let depth = nros_log::REENTRANT_DEPTH;
    let bound = 1 + depth + (depth + 1) * nros_log::early::DRAIN_PASSES;
    let lines = LINES.load(Ordering::SeqCst);
    assert!(
        lines <= bound,
        "{lines} lines from one record, past the bound of {bound}: the recursion was not bounded"
    );
}
