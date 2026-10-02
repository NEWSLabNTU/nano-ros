//! Issue 1613 -- a `log::info!` whose target nobody interned is DELIVERED
//! under that target, not under the default logger's name.
//!
//! Since issue 1324 every Zephyr Rust image routes the `log` facade through
//! `LogCrateBridge`, and every app line read `nros:` instead of `rustapp:`.
//! Observed through a real `LogSink`, because "the name was computed" and
//! "the sink received it" are different claims.

use std::sync::Mutex;

use nros_log::{LogSink, Record, Severity};

struct Collector {
    lines: Mutex<Vec<(String, Severity, String)>>,
}

impl LogSink for Collector {
    fn log(&self, record: &Record<'_>) {
        self.lines.lock().unwrap().push((
            record.logger_name.to_string(),
            record.severity,
            record.message.to_string(),
        ));
    }
}

static COLLECTOR: Collector = Collector {
    lines: Mutex::new(Vec::new()),
};

#[test]
fn un_interned_target_names_the_delivered_record() {
    assert!(
        nros_log::add_sink(&COLLECTOR),
        "add_sink refused the first sink in the program"
    );
    nros_log::log_compat::install_log_crate_bridge()
        .expect("no other log::Log may be installed in this test binary");

    log::info!(target: "rustapp", "Publishing: 'Hello World: {}'", 1);
    // No explicit target: `log` uses the module path.
    log::warn!("from the test module");

    let lines = COLLECTOR.lines.lock().unwrap().clone();
    assert!(
        lines.contains(&(
            "rustapp".to_string(),
            Severity::Info,
            "Publishing: 'Hello World: 1'".to_string()
        )),
        "the `rustapp` record was not filed under `rustapp`: {lines:?}"
    );
    assert!(
        lines.contains(&(
            module_path!().to_string(),
            Severity::Warn,
            "from the test module".to_string()
        )),
        "the default-target record was not filed under its module path: {lines:?}"
    );
    assert!(
        lines.iter().all(|(name, _, _)| name != "nros"),
        "a bridged record fell back to the default logger's NAME: {lines:?}"
    );
}
