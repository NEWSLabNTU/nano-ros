//! Phase 88.2 — the emission macros.
//!
//! Each macro formats its arguments into a stack-resident
//! [`crate::FormatBuffer`] (capacity picked at compile time by the
//! `buffer-size-<N>` feature), wraps the result in a [`crate::Record`],
//! and hands it to the [`crate::Logger`]'s dispatcher.
//!
//! Compile-time ceiling: macros below
//! [`severity_enabled_at_compile_time`](super::severity_enabled_at_compile_time)
//! expand to `()` — the format call is dead-code-eliminated.
//!
//! ## Naming (RFC-0089, settled 2026-09-04)
//!
//! The three ROS 2 client libraries disagree with each other about how to
//! spell a log line:
//!
//! ```text
//! rclrs    log_info!(logger, "…")        <- Rust follows THIS
//! rclcpp   RCLCPP_INFO(logger, "…")
//! rcutils  RCUTILS_LOG_INFO_NAMED(…)
//! ```
//!
//! There is no single "ROS 2 spelling", so each of our languages follows ITS
//! OWN upstream. For Rust that is rclrs, and rclrs's five severity macros are
//! [`log_debug!`](crate::log_debug), [`log_info!`](crate::log_info),
//! [`log_warn!`](crate::log_warn), [`log_error!`](crate::log_error) and
//! [`log_fatal!`](crate::log_fatal) — which is what this module now exports.
//! (They carry an explicit target because `#[macro_export]` puts a macro at
//! the CRATE ROOT, not in the module whose file declares it.) The pre-rename
//! ~~`nros_debug!`~~ .. ~~`nros_fatal!`~~ forwarders, and the
//! `deprecate-legacy-names` feature that armed their `#[deprecated]`, were
//! removed in phase-417 W-B5 once every in-tree call site had moved. There is
//! no log_trace!: see below.
//!
//! **A name takes rclrs's spelling exactly when rclrs HAS that name.** Two
//! families deliberately keep the `nros_` prefix, because inventing a name
//! inside rclrs's namespace-alike would claim an upstream twin that does not
//! exist:
//!
//! * [`nros_trace!`](crate::nros_trace) — rclrs stops at `debug`. TRACE is ours (ledger
//!   `rust:nros_trace`, verdict `extension`), so it keeps a name that says so.
//! * the `nros_*_throttle!` / `nros_*_throttle_at!` family — a macro per
//!   severity, which predates the modifier form below. rclrs has no such
//!   names.
//!
//! **The first argument is rclrs's [`ToLogParams`](crate::ToLogParams)**
//! (phase-515 R3), so rclrs's modifiers chain off the logger:
//! `log_info!(logger.once(), …)`, `.skip_first()`, `.only_if(b)`, and — with
//! the platform clock — `.throttle(d)`. A plain `&Logger` is still the common
//! case. See [`crate::log_params`] for the bounds.

/// Internal macro emitting one log record at `$severity`.
///
/// Use the named helpers ([`nros_trace!`](crate::nros_trace),
/// [`log_debug!`](crate::log_debug), etc.) — they
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
                timestamp_ns: $crate::__timestamp_ns(),
            };
            __logger.dispatch(&__record);
        }
    }};
}

/// Internal macro emitting one record described by a
/// [`LogParams`](crate::LogParams) — the five level macros, [`log!`] and
/// [`nros_trace!`](crate::nros_trace) all expand to this (phase-515 R3).
///
/// The occurrence and throttle state live HERE, as statics at the call site,
/// which is rclrs's arrangement too: a params value has no call-site identity
/// to hang them off. A site whose params need neither — a plain
/// `log_info!(logger, …)` — leaves both branches dead once the params are
/// constant-folded, and the statics go with them.
///
/// Order, the same as the `nros_*_throttle!` family's and for the same reason:
/// filter and severity first, so a record the level drops consumes neither
/// the `once` flag nor the throttle window; then occurrence; then the window.
#[doc(hidden)]
#[macro_export]
macro_rules! __nros_log_params_emit {
    ($params:expr, $($arg:tt)+) => {{
        let __p: $crate::LogParams<'_> = $params;
        let __logger: &$crate::Logger = __p.logger();
        let __sev = __p.get_severity();
        if __p.get_user_filter() && __logger.is_enabled(__sev) {
            static __NROS_LOG_OCCURS: $crate::log_params::OccurrenceState =
                $crate::log_params::OccurrenceState::new();
            static __NROS_LOG_THROTTLE: $crate::ThrottleState = $crate::ThrottleState::new();
            let __now = $crate::__timestamp_ns();
            let __throttle = __p.get_throttle();
            if __NROS_LOG_OCCURS.admit(__p.get_occurence())
                && (__throttle.is_zero()
                    || __NROS_LOG_THROTTLE
                        .should_log(__now, $crate::log_params::__duration_ns(__throttle)))
            {
                use ::core::fmt::Write as _;
                let mut __buf = $crate::FormatBuffer::new();
                let _ = ::core::write!(__buf, $($arg)+);
                let __record = $crate::Record {
                    severity:     __sev,
                    logger_name:  __logger.name(),
                    message:      __buf.as_str(),
                    file:         ::core::file!(),
                    line:         ::core::line!(),
                    // The reading the window was decided against — see
                    // `__nros_log_emit_throttled!`.
                    timestamp_ns: __now,
                };
                __logger.dispatch(&__record);
            }
        }
    }};
}

/// Emit one record at a severity chosen at RUNTIME — rclrs's `log!`.
///
/// phase-417 G6, ledger row `rust:log`. The severity-taking emitter existed as
/// [`__nros_log_emit!`], `#[doc(hidden)]` and named as an implementation
/// detail, so a ported `log!(logger, severity, …)` had no supported spelling
/// to land on. This is that spelling.
///
/// ```ignore
/// let severity = if recovered { Severity::Info } else { Severity::Error };
/// nros_log::log!(logger, severity, "link {}", state);
/// ```
///
/// **It routes through exactly what the five level macros route through**, and
/// that is the whole risk in a macro this cheap: the compile-time ceiling, the
/// runtime `is_enabled` threshold and the one `FormatBuffer`. If it grew its
/// own gate, `log!(logger, Severity::Debug, …)` would obey a different
/// threshold from [`log_debug!`](crate::log_debug) and the two spellings of one call would
/// disagree — which is what a second implementation of a gate always ends up
/// meaning.
///
/// MEASURED, and worth knowing before trusting the `is_enabled` half of that
/// sentence: [`crate::Logger::dispatch`] re-checks the threshold itself, so
/// removing the check inside [`__nros_log_emit!`] changes nothing observable.
/// The runtime gate is belt-and-braces; what a second implementation could
/// actually get wrong here is the COMPILE-TIME ceiling and the severity the
/// record carries, and those are what the test asserts.
///
/// **One difference from the five, forced by the runtime severity.** Those
/// consult [`crate::severity_enabled_at_compile_time`] with a CONSTANT, so a
/// call below the ceiling compiles to `()` and the formatting is
/// dead-code-eliminated. Here the severity is a value, so the same check
/// happens — at run time, on a `Severity` the compiler does not know. A call
/// site that wants the record and its arguments gone from the image entirely
/// names the level macro; this one keeps the `write!` in the binary and
/// decides per call.
///
/// The first argument is a [`ToLogParams`](crate::ToLogParams), like the
/// level macros', so `log!(logger.throttle(d), severity, …)` throttles; the
/// severity argument wins over one set in the chain.
#[macro_export]
macro_rules! log {
    ($logger:expr, $severity:expr, $($arg:tt)+) => {{
        let __sev = $severity;
        if $crate::severity_enabled_at_compile_time(__sev) {
            $crate::__nros_log_params_emit!(
                $crate::ToLogParams::severity($logger, __sev),
                $($arg)+
            );
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
            $crate::__nros_log_params_emit!(
                $crate::ToLogParams::severity($logger, $crate::Severity::Trace),
                $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Debug`].
///
/// `rclrs::log_debug!`'s spelling — see the module note on RFC-0089's
/// "each language follows its own upstream" rule. The pre-rename
/// ~~`nros_debug!`~~ forwarder was removed in phase-417 W-B5.
#[macro_export]
macro_rules! log_debug {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Debug) {
            $crate::__nros_log_params_emit!($crate::ToLogParams::debug($logger), $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Info`].
///
/// `rclrs::log_info!`'s spelling — see the module note on RFC-0089's
/// "each language follows its own upstream" rule. The pre-rename
/// ~~`nros_info!`~~ forwarder was removed in phase-417 W-B5.
#[macro_export]
macro_rules! log_info {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Info) {
            $crate::__nros_log_params_emit!($crate::ToLogParams::info($logger), $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Warn`].
///
/// `rclrs::log_warn!`'s spelling — see the module note on RFC-0089's
/// "each language follows its own upstream" rule. The pre-rename
/// ~~`nros_warn!`~~ forwarder was removed in phase-417 W-B5.
#[macro_export]
macro_rules! log_warn {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Warn) {
            $crate::__nros_log_params_emit!($crate::ToLogParams::warn($logger), $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Error`].
///
/// `rclrs::log_error!`'s spelling — see the module note on RFC-0089's
/// "each language follows its own upstream" rule. The pre-rename
/// ~~`nros_error!`~~ forwarder was removed in phase-417 W-B5.
#[macro_export]
macro_rules! log_error {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Error) {
            $crate::__nros_log_params_emit!($crate::ToLogParams::error($logger), $($arg)+);
        }
    };
}

/// Emit at [`crate::Severity::Fatal`].
///
/// `rclrs::log_fatal!`'s spelling — see the module note on RFC-0089's
/// "each language follows its own upstream" rule. The pre-rename
/// ~~`nros_fatal!`~~ forwarder was removed in phase-417 W-B5.
#[macro_export]
macro_rules! log_fatal {
    ($logger:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Fatal) {
            $crate::__nros_log_params_emit!($crate::ToLogParams::fatal($logger), $($arg)+);
        }
    };
}

// -----------------------------------------------------------------------------
// phase-417 W4.d — throttled emission.
//
// There WAS a throttle in this tree before: `nros_core::logger::Logger::
// *_throttle`. It sat on a DIFFERENT `Logger` — the one that forwards to the
// `log` crate — so it never reached `nros_platform_log_write`, and the
// `Logger` the FFI hands to C and C++ had no throttle at all. Two types, the
// capability on the unused one. The rule itself now lives in
// [`crate::throttle`], which both this macro family and the C shim call.
//
// Two spellings, and the difference is WHERE THE TIME COMES FROM:
//
// * `nros_*_throttle!(logger, interval_ms, ...)` reads the platform clock.
// * `nros_*_throttle_at!(logger, now_ns, interval_ms, ...)` takes it, which is
//   `rclcpp`'s shape (`RCLCPP_INFO_THROTTLE(logger, clock, ...)` names the
//   clock) and the only form available with no platform port.
//
// The clock-reading form REFUSES TO COMPILE without the platform clock
// (`cfg(nros_log_clock)`, issue 1037), rather than compiling into a throttle with no time base. Without a
// clock `__timestamp_ns()` is a constant `0`, and a window measured against a
// constant never elapses — the site emits its first record and then nothing,
// forever (issue 1152; before it, the same constant admitted EVERY record),
// while reading at the call site exactly like a working throttle. RFC-0089:
// never compile and differ.
//
// WHY THIS FAMILY KEEPS THE `nros_` PREFIX. rclrs has no log_info_throttle!
// to follow: its throttle is a MODIFIER on the one macro, and since phase-515
// R3 ours is too — `log_info!(logger.throttle(d), …)` through `ToLogParams`
// (`crate::log_params`). That reversed the three ledger rows this note used to
// cite against it (`rust:ToLogParams`, `rust:LogOccurrence`,
// `rust:ThrottleClock`); their objections each had an answer:
//
//   * the occurrence and throttle state live in statics the LEVEL MACRO
//     declares at the call site, as rclrs's do, not in the params value;
//   * `ThrottleClock` is bounded to `SteadyTime`, the one clock we can read;
//   * the clockless refusal survives: `ToLogParams::throttle` exists only
//     under `cfg(nros_log_clock)`, so a clockless `.throttle(d)` is a compile
//     error rather than a runtime field that silently admits everything.
//
// This family stays as our own spelling of the same capability, and the
// `_at` half stays the ONLY throttle on a clockless target, which the
// modifier form cannot express.
// -----------------------------------------------------------------------------

/// Resolves the timestamp the `nros_*_throttle!` family measures against.
///
/// With the platform clock (`cfg(nros_log_clock)` — the platform's `clock`
/// capability or the `platform-clock` feature, issue 1037): the platform's
/// monotonic clock. Without: a compile error — see the family's module note
/// for why this is not a silent degradation.
#[cfg(nros_log_clock)]
#[doc(hidden)]
#[macro_export]
macro_rules! __nros_throttle_now {
    () => {
        $crate::__timestamp_ns()
    };
}

/// See the `nros_log_clock` arm above.
#[cfg(not(nros_log_clock))]
#[doc(hidden)]
#[macro_export]
macro_rules! __nros_throttle_now {
    () => {
        ::core::compile_error!(
            "nros_*_throttle! needs a monotonic clock: build for a platform whose \
             nros-platform.toml declares `[capabilities] clock = true` (the lane names it through \
             NROS_PLATFORM_NAME), enable nros-log's `platform-clock` feature where a port is linked, \
             or use nros_*_throttle_at!(logger, now_ns, interval_ms, ...) and pass the \
             time yourself. Without a clock `__timestamp_ns()` is a constant 0, so the window \
             would admit every record while looking exactly like a working throttle."
        )
    };
}

/// Internal macro emitting one throttled record at `$severity`.
///
/// `$state` is a `&'static ThrottleState`; the public macros declare it as a
/// `static` at the call site, which is what makes the window per-SITE rather
/// than per-logger.
#[doc(hidden)]
#[macro_export]
macro_rules! __nros_log_emit_throttled {
    ($logger:expr, $severity:expr, $state:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {{
        let __logger: &$crate::Logger = $logger;
        let __sev = $severity;
        let __now: u64 = $now_ns;
        let __interval = $crate::interval_ms_to_ns($interval_ms as u64);
        // Severity first, window second — see `Logger::is_enabled_throttled`.
        // Also short-circuits the format call, like the unthrottled macros.
        if __logger.is_enabled_throttled(__sev, $state, __now, __interval) {
            use ::core::fmt::Write as _;
            let mut __buf = $crate::FormatBuffer::new();
            let _ = ::core::write!(__buf, $($arg)+);
            let __record = $crate::Record {
                severity:     __sev,
                logger_name:  __logger.name(),
                message:      __buf.as_str(),
                file:         ::core::file!(),
                line:         ::core::line!(),
                // The timestamp the window was decided against, not a second
                // reading — two clock reads for one record is how a record can
                // be stamped before the window it opened.
                timestamp_ns: __now,
            };
            __logger.dispatch(&__record);
        }
    }};
}

/// Declares the per-call-site window and forwards. Internal.
#[doc(hidden)]
#[macro_export]
macro_rules! __nros_throttle_site {
    ($logger:expr, $severity:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {{
        static __NROS_THROTTLE_SITE: $crate::ThrottleState = $crate::ThrottleState::new();
        $crate::__nros_log_emit_throttled!(
            $logger, $severity, &__NROS_THROTTLE_SITE, $now_ns, $interval_ms, $($arg)+
        );
    }};
}

/// Emit at [`crate::Severity::Trace`], at most once per `interval_ms` at this
/// call site.
///
/// Reads the platform clock; needs `cfg(nros_log_clock)` (see
/// [`nros_trace_throttle_at!`](crate::nros_trace_throttle_at) for the form that takes the time).
#[macro_export]
macro_rules! nros_trace_throttle {
    ($logger:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Trace) {
            $crate::__nros_throttle_site!(
                $logger,
                $crate::Severity::Trace,
                $crate::__nros_throttle_now!(),
                $interval_ms,
                $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Trace`], at most once per `interval_ms` at this
/// call site, measured against the `now_ns` the CALLER supplies.
///
/// `rclcpp`'s shape — `RCLCPP_TRACE_THROTTLE(logger, clock, …)` names its clock
/// too — and the only throttled form available with no platform clock.
#[macro_export]
macro_rules! nros_trace_throttle_at {
    ($logger:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Trace) {
            $crate::__nros_throttle_site!(
                $logger, $crate::Severity::Trace, $now_ns, $interval_ms, $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Debug`], at most once per `interval_ms` at this
/// call site.
///
/// Reads the platform clock; needs `cfg(nros_log_clock)` (see
/// [`nros_debug_throttle_at!`](crate::nros_debug_throttle_at) for the form that takes the time).
#[macro_export]
macro_rules! nros_debug_throttle {
    ($logger:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Debug) {
            $crate::__nros_throttle_site!(
                $logger,
                $crate::Severity::Debug,
                $crate::__nros_throttle_now!(),
                $interval_ms,
                $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Debug`], at most once per `interval_ms` at this
/// call site, measured against the `now_ns` the CALLER supplies.
///
/// `rclcpp`'s shape — `RCLCPP_DEBUG_THROTTLE(logger, clock, …)` names its clock
/// too — and the only throttled form available with no platform clock.
#[macro_export]
macro_rules! nros_debug_throttle_at {
    ($logger:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Debug) {
            $crate::__nros_throttle_site!(
                $logger, $crate::Severity::Debug, $now_ns, $interval_ms, $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Info`], at most once per `interval_ms` at this
/// call site.
///
/// Reads the platform clock; needs `cfg(nros_log_clock)` (see
/// [`nros_info_throttle_at!`](crate::nros_info_throttle_at) for the form that takes the time).
#[macro_export]
macro_rules! nros_info_throttle {
    ($logger:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Info) {
            $crate::__nros_throttle_site!(
                $logger,
                $crate::Severity::Info,
                $crate::__nros_throttle_now!(),
                $interval_ms,
                $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Info`], at most once per `interval_ms` at this
/// call site, measured against the `now_ns` the CALLER supplies.
///
/// `rclcpp`'s shape — `RCLCPP_INFO_THROTTLE(logger, clock, …)` names its clock
/// too — and the only throttled form available with no platform clock.
#[macro_export]
macro_rules! nros_info_throttle_at {
    ($logger:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Info) {
            $crate::__nros_throttle_site!(
                $logger, $crate::Severity::Info, $now_ns, $interval_ms, $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Warn`], at most once per `interval_ms` at this
/// call site.
///
/// Reads the platform clock; needs `cfg(nros_log_clock)` (see
/// [`nros_warn_throttle_at!`](crate::nros_warn_throttle_at) for the form that takes the time).
#[macro_export]
macro_rules! nros_warn_throttle {
    ($logger:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Warn) {
            $crate::__nros_throttle_site!(
                $logger,
                $crate::Severity::Warn,
                $crate::__nros_throttle_now!(),
                $interval_ms,
                $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Warn`], at most once per `interval_ms` at this
/// call site, measured against the `now_ns` the CALLER supplies.
///
/// `rclcpp`'s shape — `RCLCPP_WARN_THROTTLE(logger, clock, …)` names its clock
/// too — and the only throttled form available with no platform clock.
#[macro_export]
macro_rules! nros_warn_throttle_at {
    ($logger:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Warn) {
            $crate::__nros_throttle_site!(
                $logger, $crate::Severity::Warn, $now_ns, $interval_ms, $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Error`], at most once per `interval_ms` at this
/// call site.
///
/// Reads the platform clock; needs `cfg(nros_log_clock)` (see
/// [`nros_error_throttle_at!`](crate::nros_error_throttle_at) for the form that takes the time).
#[macro_export]
macro_rules! nros_error_throttle {
    ($logger:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Error) {
            $crate::__nros_throttle_site!(
                $logger,
                $crate::Severity::Error,
                $crate::__nros_throttle_now!(),
                $interval_ms,
                $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Error`], at most once per `interval_ms` at this
/// call site, measured against the `now_ns` the CALLER supplies.
///
/// `rclcpp`'s shape — `RCLCPP_ERROR_THROTTLE(logger, clock, …)` names its clock
/// too — and the only throttled form available with no platform clock.
#[macro_export]
macro_rules! nros_error_throttle_at {
    ($logger:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Error) {
            $crate::__nros_throttle_site!(
                $logger, $crate::Severity::Error, $now_ns, $interval_ms, $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Fatal`], at most once per `interval_ms` at this
/// call site.
///
/// Reads the platform clock; needs `cfg(nros_log_clock)` (see
/// [`nros_fatal_throttle_at!`](crate::nros_fatal_throttle_at) for the form that takes the time).
#[macro_export]
macro_rules! nros_fatal_throttle {
    ($logger:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Fatal) {
            $crate::__nros_throttle_site!(
                $logger,
                $crate::Severity::Fatal,
                $crate::__nros_throttle_now!(),
                $interval_ms,
                $($arg)+
            );
        }
    };
}

/// Emit at [`crate::Severity::Fatal`], at most once per `interval_ms` at this
/// call site, measured against the `now_ns` the CALLER supplies.
///
/// `rclcpp`'s shape — `RCLCPP_FATAL_THROTTLE(logger, clock, …)` names its clock
/// too — and the only throttled form available with no platform clock.
#[macro_export]
macro_rules! nros_fatal_throttle_at {
    ($logger:expr, $now_ns:expr, $interval_ms:expr, $($arg:tt)+) => {
        if $crate::severity_enabled_at_compile_time($crate::Severity::Fatal) {
            $crate::__nros_throttle_site!(
                $logger, $crate::Severity::Fatal, $now_ns, $interval_ms, $($arg)+
            );
        }
    };
}
