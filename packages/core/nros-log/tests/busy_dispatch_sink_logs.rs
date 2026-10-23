//! Issue 2000 — a sink that logs from inside `log()` is postponed, and the
//! postponement is bounded: no recursion, no loop.
//!
//! Its own test binary: the dispatch guard and the hold queue are
//! process-wide, and this test deliberately leaves a record in the queue.

use std::sync::Mutex;

use nros_log::{LogSink, Logger, Record, Severity, init};

static LOGGER: Logger = Logger::with_level("sink_logs", Severity::Trace);

struct Echo {
    lines: Mutex<Vec<String>>,
}

impl LogSink for Echo {
    fn log(&self, record: &Record<'_>) {
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(record.message.to_string());
        // The worst case: a sink that logs on EVERY write, including the
        // writes of its own records.
        nros_log::log_info!(&LOGGER, "echo");
    }
}

static ECHO: Echo = Echo {
    lines: Mutex::new(Vec::new()),
};
static SINKS: [&dyn LogSink; 1] = [&ECHO];

#[test]
fn a_sink_that_always_logs_adds_a_bounded_number_of_lines() {
    init(&SINKS);

    nros_log::log_info!(&LOGGER, "first");

    let echoes = {
        let lines = ECHO.lines.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(lines.first().map(String::as_str), Some("first"));
        lines.iter().filter(|l| *l == "echo").count()
    };
    // One echo per drain pass: each pass delivers what was held when it began,
    // and each delivery holds one more. The last one waits.
    assert_eq!(
        echoes,
        nros_log::early::DRAIN_PASSES,
        "a sink's own records must be delivered, one per pass, and no more"
    );

    // The record left over is delivered by the next dispatch, not lost.
    nros_log::log_info!(&LOGGER, "second");
    let lines = ECHO.lines.lock().unwrap_or_else(|e| e.into_inner());
    let second = lines.iter().position(|l| l == "second").expect("second");
    assert!(
        lines[second + 1..].iter().any(|l| l == "echo"),
        "the held echo must follow the next record: {lines:?}"
    );
    assert_eq!(nros_log::early::overflowed(), 0);
}
