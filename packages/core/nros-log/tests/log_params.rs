//! phase-515 R3 — rclrs's log modifiers on the level macros:
//! `log_info!(logger.once(), …)`, `.skip_first()`, `.only_if(b)`, and the
//! severity-taking `log!` over the same params. The clock-reading
//! `.throttle(d)` is `log_params_throttle.rs`, which needs the platform clock.
//!
//! Delivery is observed through a real sink, as in
//! `named_loggers_levels_and_throttle.rs`, and the tests that assert on what
//! was collected serialise on one lock for the reason that file gives.

use std::sync::Mutex;

use nros_log::{
    LogOccurrence, LogSink, Logger, Record, Severity, ToLogParams, log, log_info, log_warn,
};

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
static DISPATCH: Mutex<()> = Mutex::new(());

fn install_collector() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| assert!(nros_log::add_sink(&COLLECTOR)));
}

fn drain_for(logger_name: &str) -> Vec<(Severity, String)> {
    let mut lines = COLLECTOR.lines.lock().unwrap();
    let mine = lines
        .iter()
        .filter(|(name, ..)| name == logger_name)
        .map(|(_, sev, msg)| (*sev, msg.clone()))
        .collect();
    lines.retain(|(name, ..)| name != logger_name);
    mine
}

fn messages(lines: &[(Severity, String)]) -> Vec<&str> {
    lines.iter().map(|(_, m)| m.as_str()).collect()
}

#[test]
fn once_emits_the_first_record_of_its_site_only() {
    let _g = DISPATCH.lock().unwrap();
    install_collector();
    static L: Logger = Logger::with_level("r3_once", Severity::Debug);
    for i in 0..4 {
        log_info!(L.once(), "tick {i}");
    }
    assert_eq!(messages(&drain_for("r3_once")), ["tick 0"]);
}

#[test]
fn skip_first_emits_every_record_but_the_first() {
    let _g = DISPATCH.lock().unwrap();
    install_collector();
    static L: Logger = Logger::with_level("r3_skip", Severity::Debug);
    for i in 0..4 {
        log_warn!(L.skip_first(), "tick {i}");
    }
    assert_eq!(
        messages(&drain_for("r3_skip")),
        ["tick 1", "tick 2", "tick 3"]
    );
}

/// The state is per CALL SITE, as rclrs's is: two `once` sites on one logger
/// each emit once.
#[test]
fn each_call_site_has_its_own_once() {
    let _g = DISPATCH.lock().unwrap();
    install_collector();
    static L: Logger = Logger::with_level("r3_sites", Severity::Debug);
    for _ in 0..3 {
        log_info!(L.once(), "site a");
        log_info!(L.once(), "site b");
    }
    assert_eq!(messages(&drain_for("r3_sites")), ["site a", "site b"]);
}

/// A record the level filters, or the user filter drops, must not consume the
/// site's `once` — otherwise raising the level later loses the one record.
#[test]
fn a_filtered_record_does_not_consume_once() {
    let _g = DISPATCH.lock().unwrap();
    install_collector();
    static L: Logger = Logger::with_level("r3_filtered", Severity::Warn);
    let emit = |pass: u32, enabled: bool| {
        log_info!(L.once().only_if(enabled), "pass {pass}");
    };
    emit(0, true); // below Warn: filtered by level
    emit(1, false); // dropped by only_if
    L.set_level(Severity::Debug);
    emit(2, false); // dropped by only_if
    emit(3, true); // the first record that passes: emits
    emit(4, true); // once already spent
    assert_eq!(messages(&drain_for("r3_filtered")), ["pass 3"]);
}

#[test]
fn only_if_conditions_and_together() {
    let _g = DISPATCH.lock().unwrap();
    install_collector();
    static L: Logger = Logger::with_level("r3_only_if", Severity::Debug);
    for (a, b) in [(true, true), (true, false), (false, true), (false, false)] {
        log_info!(L.only_if(a).only_if(b), "{a} {b}");
    }
    assert_eq!(messages(&drain_for("r3_only_if")), ["true true"]);
}

/// `log!` takes params too; its severity argument is the record's.
#[test]
fn log_takes_params_and_its_severity_wins() {
    let _g = DISPATCH.lock().unwrap();
    install_collector();
    static L: Logger = Logger::with_level("r3_log", Severity::Debug);
    for i in 0..2 {
        log!(L.error().once(), Severity::Warn, "tick {i}");
    }
    assert_eq!(
        drain_for("r3_log"),
        [(Severity::Warn, String::from("tick 0"))]
    );
}

/// The two spellings the macros accepted before they took `ToLogParams` still
/// compile and deliver: a `&Logger`, and a reference to one.
#[test]
fn a_logger_and_a_reference_to_one_still_work() {
    let _g = DISPATCH.lock().unwrap();
    install_collector();
    static L: Logger = Logger::with_level("r3_plain", Severity::Debug);
    let logger: &Logger = &L;
    log_info!(logger, "plain");
    log_info!(&logger, "by reference");
    assert_eq!(messages(&drain_for("r3_plain")), ["plain", "by reference"]);
}

#[test]
fn params_report_what_was_chained() {
    static L: Logger = Logger::new("r3_getters");
    let p = (&L).skip_first().warn().only_if(false);
    assert_eq!(p.get_logger_name(), "r3_getters");
    assert_eq!(p.get_severity(), Severity::Warn);
    assert_eq!(p.get_occurence(), LogOccurrence::SkipFirst);
    assert!(p.get_throttle().is_zero());
    assert!(!p.get_user_filter());
}
