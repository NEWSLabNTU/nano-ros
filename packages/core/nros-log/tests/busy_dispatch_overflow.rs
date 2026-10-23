//! Issue 2000 — when more records wait for a busy dispatcher than the hold
//! queue has slots, the loss is counted and reported through the sinks.
//!
//! Its own test binary: the loss counter is process-wide and never reset.

use std::sync::Mutex;

use nros_log::{LogSink, Logger, Record, Severity, init};

static LOGGER: Logger = Logger::with_level("overflow", Severity::Trace);

struct Flood {
    lines: Mutex<Vec<String>>,
}

const EXTRA: usize = 3;

impl LogSink for Flood {
    fn log(&self, record: &Record<'_>) {
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(format!("{}|{}", record.logger_name, record.message));
        if record.message == "flood" {
            for i in 0..nros_log::early::early_depth() + EXTRA {
                nros_log::log_info!(&LOGGER, "held {}", i);
            }
        }
    }
}

static FLOOD: Flood = Flood {
    lines: Mutex::new(Vec::new()),
};
static SINKS: [&dyn LogSink; 1] = [&FLOOD];

#[test]
fn records_that_do_not_fit_are_counted_and_reported() {
    init(&SINKS);

    nros_log::log_info!(&LOGGER, "flood");

    assert_eq!(nros_log::early::overflowed(), EXTRA);
    let lines = FLOOD.lines.lock().unwrap_or_else(|e| e.into_inner());
    let held = lines
        .iter()
        .filter(|l| l.starts_with("overflow|held "))
        .count();
    assert_eq!(
        held,
        nros_log::early::early_depth(),
        "every record that fit must be delivered: {lines:?}"
    );
    let expected = format!("nros_log|{EXTRA} log record(s) lost:");
    assert_eq!(
        lines.iter().filter(|l| l.starts_with(&expected)).count(),
        1,
        "the loss must be reported once: {lines:?}"
    );
}
