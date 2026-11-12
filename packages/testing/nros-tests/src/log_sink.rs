//! A stderr sink for `nros_log`, for host tests (issue 2457).
//!
//! The runtime reports WHY an operation failed through `nros_log`: a zenoh
//! session open that gets no session logs the locator, the connect budget and
//! the failure class (`report_open_budget_spent`). `nros_log` dispatches only to
//! sinks installed with `nros_log::init`, and a host integration test installs
//! none, so that line went nowhere. A merge-queue run printed only
//! `open: ConnectionFailed`, and nobody could say which phase failed.
//!
//! [`install_stderr_log_sink`] installs one sink that writes each record to
//! stderr, which nextest keeps for a failing test. The router fixture calls it
//! (`ZenohRouter`'s readiness wait), so every test that starts a router gets
//! it. A test that installs its own sinks (the capturing sinks of
//! `tests/logging.rs`) starts no router and is not affected. A test that ever
//! does both must install its own sinks AFTER starting the router, because
//! `nros_log::init` replaces the list.
//!
//! Records raised before the first install are held by `nros_log` and
//! delivered by `init`, so a record from before the router started is not lost.

use std::sync::Once;

use nros_log::{LogSink, Record};

struct StderrSink;

impl LogSink for StderrSink {
    fn log(&self, record: &Record<'_>) {
        eprintln!(
            "[nros_log {}] {}: {}",
            record.severity.as_str(),
            record.logger_name,
            record.message
        );
    }
}

static STDERR_SINK: StderrSink = StderrSink;
static SINKS: &[&dyn LogSink] = &[&STDERR_SINK];

/// Install the stderr sink as `nros_log`'s sink list. Idempotent: only the
/// first call in a process installs it.
pub fn install_stderr_log_sink() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| nros_log::init(SINKS));
}
