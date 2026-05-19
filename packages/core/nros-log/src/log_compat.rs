//! Phase 88.10 — `log` crate interop.
//!
//! Two bridges, both gated behind the `log-compat` Cargo feature:
//!
//! 1. [`LogCrateSink`] — a [`crate::LogSink`] that forwards records
//!    to the `log` crate. Lets nros-log call sites reach `log`-
//!    crate listeners (env_logger / fern / pretty_env_logger / …).
//!
//! 2. [`install_log_crate_bridge`] — installs an `impl log::Log`
//!    that forwards `log::info!` etc. calls back into nros-log's
//!    dispatcher. Lets ecosystem crates that target `log::info!`
//!    flow through the same sink list.
//!
//! Either / both can be active. Cycles are short-circuited by the
//! facade's process-global recursion guard.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::{LogSink, Record, Severity};

#[cfg(test)]
use crate::{DEFAULT_LOGGER, Logger};

/// Convert nros-log [`Severity`] to `log::Level`.
///
/// `log` has no Trace/Fatal sentinels. Trace folds into the `Trace`
/// level (closest); Fatal folds into `Error`.
#[must_use]
pub fn severity_to_log_level(s: Severity) -> log::Level {
    match s {
        Severity::Trace => log::Level::Trace,
        Severity::Debug => log::Level::Debug,
        Severity::Info => log::Level::Info,
        Severity::Warn => log::Level::Warn,
        Severity::Error | Severity::Fatal => log::Level::Error,
    }
}

/// Convert `log::Level` to nros-log [`Severity`].
///
/// `log` has no Fatal; map nothing onto it.
#[must_use]
pub fn log_level_to_severity(level: log::Level) -> Severity {
    match level {
        log::Level::Trace => Severity::Trace,
        log::Level::Debug => Severity::Debug,
        log::Level::Info => Severity::Info,
        log::Level::Warn => Severity::Warn,
        log::Level::Error => Severity::Error,
    }
}

// =============================================================================
// Bridge #1: nros-log → log crate
// =============================================================================

/// A [`LogSink`] that re-emits each record through the `log` crate.
///
/// Add to the sink list passed to [`crate::init`]:
///
/// ```ignore
/// static SINKS: &[&dyn nros_log::LogSink] = &[
///     &nros_log::sinks::PlatformSink,
///     &nros_log::log_compat::LogCrateSink,
/// ];
/// nros_log::init(SINKS);
/// ```
///
/// Records re-emitted this way carry the original severity (mapped
/// via [`severity_to_log_level`]); the `log::Record::target` is the
/// nros-log [`Logger::name`].
pub struct LogCrateSink;

impl LogSink for LogCrateSink {
    fn log(&self, record: &Record<'_>) {
        let level = severity_to_log_level(record.severity);
        if !log::log_enabled!(target: record.logger_name, level) {
            return;
        }
        log::logger().log(
            &log::Record::builder()
                .args(format_args!("{}", record.message))
                .level(level)
                .target(record.logger_name)
                .file(Some(record.file))
                .line(Some(record.line))
                .build(),
        );
    }

    fn flush(&self) {
        log::logger().flush();
    }
}

// =============================================================================
// Bridge #2: log crate → nros-log
// =============================================================================

/// A `log::Log` impl that forwards every record to nros-log's
/// dispatcher.
///
/// Looks up an intern'd nros-log [`Logger`] keyed on
/// `log::Record::target` (the canonical `log` crate per-call-site
/// name); falls back to [`DEFAULT_LOGGER`] when not registered.
pub struct LogCrateBridge;

impl log::Log for LogCrateBridge {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        let sev = log_level_to_severity(metadata.level());
        if !crate::severity_enabled_at_compile_time(sev) {
            return false;
        }
        let logger = crate::get_logger(metadata.target());
        logger.is_enabled(sev)
    }

    fn log(&self, record: &log::Record<'_>) {
        let sev = log_level_to_severity(record.level());
        if !crate::severity_enabled_at_compile_time(sev) {
            return;
        }
        let logger = crate::get_logger(record.target());
        if !logger.is_enabled(sev) {
            return;
        }
        use core::fmt::Write as _;
        let mut buf = crate::FormatBuffer::new();
        let _ = core::write!(&mut buf, "{}", record.args());
        // `Record::file` is `&'static str`; `log::Record::file` is
        // `Option<&'a str>`. Drop the dynamic file/line to keep
        // the lifetime story simple — the message body still
        // identifies the call site via Display formatting if the
        // caller embedded it.
        let nros_record = Record {
            severity: sev,
            logger_name: logger.name(),
            message: buf.as_str(),
            file: "<log-compat-bridge>",
            line: 0,
            timestamp_ns: 0,
        };
        logger.dispatch(&nros_record);
    }

    fn flush(&self) {
        crate::flush();
    }
}

static BRIDGE_INSTALLED: AtomicBool = AtomicBool::new(false);
static BRIDGE: LogCrateBridge = LogCrateBridge;

/// Install [`LogCrateBridge`] as the process-wide `log::Log` impl.
///
/// Idempotent — re-installation is a no-op (the `log` crate forbids
/// replacement after first set). Also sets the `log::LevelFilter`
/// to `Trace` so the bridge sees every macro emission; downstream
/// filtering happens in nros-log's per-Logger thresholds.
///
/// Returns `Err(log::SetLoggerError)` if the process already
/// installed a different `log::Log` impl.
pub fn install_log_crate_bridge() -> Result<(), log::SetLoggerError> {
    if BRIDGE_INSTALLED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    log::set_logger(&BRIDGE)?;
    log::set_max_level(log::LevelFilter::Trace);
    Ok(())
}

// =============================================================================
// Tests (host-only, behind `log-compat`).
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_maps_round_trip_for_common_levels() {
        for (sev, level) in [
            (Severity::Trace, log::Level::Trace),
            (Severity::Debug, log::Level::Debug),
            (Severity::Info, log::Level::Info),
            (Severity::Warn, log::Level::Warn),
            (Severity::Error, log::Level::Error),
        ] {
            assert_eq!(severity_to_log_level(sev), level);
            assert_eq!(log_level_to_severity(level), sev);
        }
        // Fatal folds into Error one-way.
        assert_eq!(severity_to_log_level(Severity::Fatal), log::Level::Error);
    }

    #[test]
    fn bridge_dispatch_uses_default_logger_for_unknown_target() {
        use log::Log as _;
        let _ = &DEFAULT_LOGGER;
        let _ = core::any::TypeId::of::<Logger>(); // silence unused-import on `Logger`
        let metadata = log::Metadata::builder()
            .level(log::Level::Info)
            .target("totally-unregistered-target")
            .build();
        assert!(BRIDGE.enabled(&metadata));
    }
}
