//! rclrs's log modifiers — `log_info!(logger.once(), …)`,
//! `.skip_first()`, `.throttle(d)`, `.only_if(b)` (phase-515 R3).
//!
//! rclrs's five level macros take anything implementing [`ToLogParams`] as
//! their first argument, so a modifier chains off the logger:
//!
//! ```ignore
//! log_info!(logger.once(), "link up");
//! log_warn!(logger.skip_first().throttle(Duration::from_millis(500)), "lag {}", ms);
//! log_debug!(logger.only_if(verbose), "state {:?}", s);
//! ```
//!
//! ADOPT-BOUNDED. rclrs's names, with three stated differences:
//!
//! * [`ToLogParams`] is implemented for `&Logger` and [`LogParams`] only. rclrs
//!   also takes a node or a logger NAME; here a node's logger is
//!   `node.logger()`, and a name is [`crate::get_logger`].
//! * [`ThrottleClock`] has one variant, `SteadyTime`: the platform's monotonic
//!   clock is the only clock the logging path can read. rclrs's `SystemTime`
//!   and `Clock(…)` have nothing to name here.
//! * [`ToLogParams::throttle`] and [`ToLogParams::throttle_clock`] exist only
//!   when the image has that clock (`cfg(nros_log_clock)`, issue 1037). Without
//!   one the window would be measured against a constant and the call site
//!   would read like a working throttle while emitting once and then never
//!   (issue 1152), so a clockless build refuses `.throttle(d)` at compile time
//!   — the same refusal the `nros_*_throttle!` family makes, and the
//!   `nros_*_throttle_at!` family stays the clockless form.
//!
//! The state `once`, `skip_first` and `throttle` need is NOT in the params
//! value. As in rclrs, the level macro declares it as statics at the call site
//! ([`crate::__nros_log_params_emit!`]), which is what makes it per site. A
//! call site whose params are known at compile time to need neither — every
//! plain `log_info!(logger, …)` — references neither static once optimised,
//! and they are dropped with the dead branch.

use core::time::Duration;

use crate::{Logger, Severity};

/// rclrs's `LogOccurrence`: which records of a call site emit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogOccurrence {
    /// Every record (the default).
    #[default]
    All,
    /// Only the first record that passes the severity and filter checks.
    Once,
    /// Every record except the first that passes those checks.
    SkipFirst,
}

/// rclrs's `ThrottleClock`: the clock a throttle window is measured on.
///
/// ADOPT-BOUNDED: `SteadyTime` only — the platform's monotonic clock is the
/// one the logging path can read. See the module note.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThrottleClock {
    /// The platform's monotonic clock (`nros_platform_clock_ns`).
    #[default]
    SteadyTime,
}

/// rclrs's `LogParams`: the logger plus the modifiers one record carries.
///
/// ADOPT-BOUNDED: it borrows the [`Logger`] (`LogParams<'a>`) where rclrs's
/// owns a logger name. Build one through [`ToLogParams`].
#[derive(Clone, Copy)]
pub struct LogParams<'a> {
    logger: &'a Logger,
    severity: Severity,
    occurs: LogOccurrence,
    throttle: Duration,
    throttle_clock: ThrottleClock,
    only_if: bool,
}

impl<'a> LogParams<'a> {
    /// rclrs's `LogParams::new`: `logger`, at `Info`, every record, no
    /// throttle, no filter.
    #[must_use]
    pub const fn new(logger: &'a Logger) -> Self {
        Self {
            logger,
            severity: Severity::Info,
            occurs: LogOccurrence::All,
            throttle: Duration::ZERO,
            throttle_clock: ThrottleClock::SteadyTime,
            only_if: true,
        }
    }

    /// The logger the record goes to.
    #[must_use]
    pub const fn logger(&self) -> &'a Logger {
        self.logger
    }

    /// rclrs's `get_logger_name`: the logger's name.
    #[must_use]
    pub const fn get_logger_name(&self) -> &'static str {
        self.logger.name()
    }

    /// rclrs's `get_severity`.
    #[must_use]
    pub const fn get_severity(&self) -> Severity {
        self.severity
    }

    /// rclrs's `get_occurence` (rclrs's spelling, kept).
    #[must_use]
    pub const fn get_occurence(&self) -> LogOccurrence {
        self.occurs
    }

    /// rclrs's `get_throttle`: `Duration::ZERO` for no throttle.
    #[must_use]
    pub const fn get_throttle(&self) -> Duration {
        self.throttle
    }

    /// rclrs's `get_throttle_clock`.
    #[must_use]
    pub const fn get_throttle_clock(&self) -> ThrottleClock {
        self.throttle_clock
    }

    /// rclrs's `get_user_filter`: the `only_if` condition.
    #[must_use]
    pub const fn get_user_filter(&self) -> bool {
        self.only_if
    }
}

/// rclrs's `ToLogParams`: anything a level macro takes as its first argument,
/// and the modifiers that chain off it.
///
/// Implemented for `&Logger` and [`LogParams`]; see the module note.
pub trait ToLogParams<'a>: Sized {
    /// Convert to params with this value's settings.
    fn to_log_params(self) -> LogParams<'a>;

    /// Emit only the first record of this call site.
    fn once(self) -> LogParams<'a> {
        self.occurs(LogOccurrence::Once)
    }

    /// Emit every record of this call site except the first.
    fn skip_first(self) -> LogParams<'a> {
        self.occurs(LogOccurrence::SkipFirst)
    }

    /// Set the occurrence rule.
    fn occurs(self, occurs: LogOccurrence) -> LogParams<'a> {
        let mut p = self.to_log_params();
        p.occurs = occurs;
        p
    }

    /// Emit at most one record of this call site per `throttle`, measured
    /// from the last record emitted. `Duration::ZERO` is no throttle.
    ///
    /// Present only with the platform clock — see the module note.
    #[cfg(nros_log_clock)]
    fn throttle(self, throttle: Duration) -> LogParams<'a> {
        let mut p = self.to_log_params();
        p.throttle = throttle;
        p
    }

    /// Set the clock a throttle is measured on.
    ///
    /// Present only with the platform clock — see the module note.
    #[cfg(nros_log_clock)]
    fn throttle_clock(self, clock: ThrottleClock) -> LogParams<'a> {
        let mut p = self.to_log_params();
        p.throttle_clock = clock;
        p
    }

    /// Emit only while `only_if` is true. Combines with an earlier filter by
    /// AND, so a chain of conditions all have to hold.
    fn only_if(self, only_if: bool) -> LogParams<'a> {
        let mut p = self.to_log_params();
        p.only_if = p.only_if && only_if;
        p
    }

    /// At `Debug`.
    fn debug(self) -> LogParams<'a> {
        self.severity(Severity::Debug)
    }

    /// At `Info`.
    fn info(self) -> LogParams<'a> {
        self.severity(Severity::Info)
    }

    /// At `Warn`.
    fn warn(self) -> LogParams<'a> {
        self.severity(Severity::Warn)
    }

    /// At `Error`.
    fn error(self) -> LogParams<'a> {
        self.severity(Severity::Error)
    }

    /// At `Fatal`.
    fn fatal(self) -> LogParams<'a> {
        self.severity(Severity::Fatal)
    }

    /// At `severity`.
    fn severity(self, severity: Severity) -> LogParams<'a> {
        let mut p = self.to_log_params();
        p.severity = severity;
        p
    }
}

impl<'a> ToLogParams<'a> for &'a Logger {
    fn to_log_params(self) -> LogParams<'a> {
        LogParams::new(self)
    }
}

/// `log_info!(&logger, …)` where `logger` is already a `&Logger` — the
/// spelling the level macros accepted by deref coercion before they took
/// [`ToLogParams`].
impl<'a, 'b: 'a> ToLogParams<'a> for &'a &'b Logger {
    fn to_log_params(self) -> LogParams<'a> {
        LogParams::new(self)
    }
}

impl<'a> ToLogParams<'a> for LogParams<'a> {
    fn to_log_params(self) -> LogParams<'a> {
        self
    }
}

/// Per-call-site state of `once` / `skip_first`. Internal: the level macros
/// declare one as a `static` at each site.
#[doc(hidden)]
pub struct OccurrenceState(portable_atomic::AtomicBool);

impl OccurrenceState {
    #[doc(hidden)]
    #[must_use]
    pub const fn new() -> Self {
        Self(portable_atomic::AtomicBool::new(false))
    }

    /// Whether a record that passed every other check emits under `occurs`.
    /// Marks the site seen on its first such record.
    #[doc(hidden)]
    pub fn admit(&self, occurs: LogOccurrence) -> bool {
        match occurs {
            LogOccurrence::All => true,
            LogOccurrence::Once => !self.0.swap(true, portable_atomic::Ordering::AcqRel),
            LogOccurrence::SkipFirst => self.0.swap(true, portable_atomic::Ordering::AcqRel),
        }
    }
}

impl Default for OccurrenceState {
    fn default() -> Self {
        Self::new()
    }
}

/// Duration to the nanoseconds a [`crate::ThrottleState`] measures, saturating.
#[doc(hidden)]
#[must_use]
pub const fn __duration_ns(d: Duration) -> u64 {
    let ns = d.as_nanos();
    if ns > u64::MAX as u128 {
        u64::MAX
    } else {
        ns as u64
    }
}
