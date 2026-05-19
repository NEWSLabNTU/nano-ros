//! Phase 88.2 — `nros_*!` macros.
//!
//! Each macro formats its arguments into a stack-resident
//! [`crate::FormatBuffer`] (capacity picked at compile time by the
//! `buffer-size-<N>` feature), wraps the result in a [`crate::Record`],
//! and hands it to the [`crate::Logger`]'s dispatcher.
//!
//! Compile-time ceiling: macros below
//! [`crate::compile_time_ceiling`](super::severity_enabled_at_compile_time)
//! expand to `()` — the format call is dead-code-eliminated.

/// Internal macro emitting one log record at `$severity`.
///
/// Use the named helpers ([`nros_trace!`], [`nros_debug!`], etc.) — they
/// gate on the compile-time ceiling before evaluating this body.
#[doc(hidden)]
#[macro_export]
macro_rules! __nros_log_emit {
    ($logger:expr, $severity:expr, $($arg:tt)+) => {{
        // Cheap runtime threshold check first to short-circuit
        // disabled call sites before formatting.
        let __logger: &$crate::Logger = $logger;
        let __sev = $severity;
        if __logger.is_enabled(__sev) {
            use ::core::fmt::Write as _;
            let mut __buf = $crate::FormatBuffer::new();
            // Ignoring `write!` result — `FormatBuffer::write_str`
            // never returns `Err`; overflow is signalled via the
            // `truncated()` accessor (not the `Result`).
            let _ = ::core::write!(__buf, $($arg)+);
            let __record = $crate::Record {
                severity:     __sev,
                logger_name:  __logger.name(),
                message:      __buf.as_str(),
                file:         ::core::file!(),
                line:         ::core::line!(),
                timestamp_ns: 0,
            };
            __logger.dispatch(&__record);
        }
    }};
}

/// Emit at [`crate::Severity::Trace`].
///
/// Disabled at compile time unless the `max-level-trace` feature is
/// the active ceiling (default).
#[macro_export]
macro_rules! nros_trace {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Trace) {
            $crate::__nros_log_emit!($logger, $crate::Severity::Trace, $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Debug`].
#[macro_export]
macro_rules! nros_debug {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Debug) {
            $crate::__nros_log_emit!($logger, $crate::Severity::Debug, $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Info`].
#[macro_export]
macro_rules! nros_info {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Info) {
            $crate::__nros_log_emit!($logger, $crate::Severity::Info, $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Warn`].
#[macro_export]
macro_rules! nros_warn {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Warn) {
            $crate::__nros_log_emit!($logger, $crate::Severity::Warn, $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Error`].
#[macro_export]
macro_rules! nros_error {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Error) {
            $crate::__nros_log_emit!($logger, $crate::Severity::Error, $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Fatal`].
#[macro_export]
macro_rules! nros_fatal {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Fatal) {
            $crate::__nros_log_emit!($logger, $crate::Severity::Fatal, $($arg)+);
        }
    };
}
