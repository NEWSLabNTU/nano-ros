//! Phase 88 — portable leveled-logging facade for nano-ros.
//!
//! See [`docs/roadmap/archived/phase-88-nros-log.md`](../../../docs/roadmap/archived/phase-88-nros-log.md)
//! for the design and acceptance criteria.
//!
//! ## Layering
//!
//! - This crate carries only the portable types + dispatcher +
//!   macros + `PlatformSink`. No backend code.
//! - Per-platform log delivery is the responsibility of each
//!   `nros-platform-<rtos>` crate, exposing
//!   `nros_platform_log_write` / `nros_platform_log_flush` via the
//!   `nros_platform_*` ABI (header at
//!   `packages/core/nros-platform-api/include/nros/platform.h`).
//! - `PlatformSink` is the bridge: a single `LogSink` impl that
//!   forwards to the ABI. Apps that want fan-out (e.g.
//!   `Platform + /rosout`) compose a `&'static [&dyn LogSink]`
//!   manually and pass it to [`init`].
//!
//! ## Quick start
//!
//! ```ignore
//! use nros_log::{Logger, Severity};
//! use nros_log::{nros_info, nros_warn};
//!
//! static LOGGER: Logger = Logger::new("my_node");
//!
//! fn main() {
//!     nros_log::register_logger(&LOGGER);
//!     nros_log::init(nros_log::sinks::default());
//!     nros_info!(&LOGGER, "started; domain = {}", 42);
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

#[cfg(feature = "alloc")]
extern crate alloc;

// Phase 88.16.E — portable-atomic polyfill for CAS-less targets
// (RISC-V `imc`, etc.). Feature unification: a consuming bare-metal
// crate enables `unsafe-assume-single-core` / `critical-section` on
// its own `portable-atomic` dep; native CAS targets get the
// passthrough.
use portable_atomic::{AtomicPtr, AtomicU8, Ordering};

#[cfg(feature = "log-compat")]
pub mod log_compat;
pub mod macros;
pub mod sinks;

mod buffer;

pub use buffer::{FormatBuffer, format_buffer_capacity};

/// REP-2012 severity levels, mirroring `rcutils_log_severity_t`.
///
/// The integer representation is stable and part of the ABI for
/// `nros_platform_log_write`. Lower value = more verbose.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Severity {
    /// Per-instruction granularity. Off unless `max-level-trace` is
    /// the active ceiling.
    Trace = 0,
    /// Diagnostic information useful while developing.
    Debug = 1,
    /// Normal operation events worth surfacing once.
    Info = 2,
    /// Unexpected but recoverable conditions.
    Warn = 3,
    /// Errors the caller should surface; the system continues.
    Error = 4,
    /// Unrecoverable — the system is about to abort.
    Fatal = 5,
}

impl Severity {
    /// Short uppercase label suitable for log-line rendering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }

    /// Stable `u8` discriminant for cross-ABI use.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Reconstruct a [`Severity`] from its `u8` discriminant.
///
/// Returns `None` for `> 5`.
#[must_use]
pub const fn severity_from_u8(value: u8) -> Option<Severity> {
    match value {
        0 => Some(Severity::Trace),
        1 => Some(Severity::Debug),
        2 => Some(Severity::Info),
        3 => Some(Severity::Warn),
        4 => Some(Severity::Error),
        5 => Some(Severity::Fatal),
        _ => None,
    }
}

/// Compile-time ceiling check used by the `nros_*!` macros.
///
/// Returns `true` iff `severity` is allowed under the configured
/// `max-level-*` feature.
#[must_use]
pub const fn severity_enabled_at_compile_time(severity: Severity) -> bool {
    if cfg!(feature = "max-level-off") {
        return false;
    }
    let ceiling = compile_time_ceiling();
    (severity as u8) >= (ceiling as u8)
}

const fn compile_time_ceiling() -> Severity {
    if cfg!(feature = "max-level-trace") {
        Severity::Trace
    } else if cfg!(feature = "max-level-debug") {
        Severity::Debug
    } else if cfg!(feature = "max-level-info") {
        Severity::Info
    } else if cfg!(feature = "max-level-warn") {
        Severity::Warn
    } else if cfg!(feature = "max-level-error") {
        Severity::Error
    } else {
        // No ceiling feature = treat as `max-level-trace`.
        Severity::Trace
    }
}

/// One log entry, handed to each [`LogSink`].
///
/// `message` is already formatted — sinks must NOT re-format.
#[derive(Debug)]
pub struct Record<'a> {
    /// Severity of the record.
    pub severity: Severity,
    /// Name of the originating [`Logger`].
    pub logger_name: &'a str,
    /// Formatted message text (no trailing newline).
    pub message: &'a str,
    /// File the macro invocation came from (`core::file!()`).
    pub file: &'static str,
    /// Line within `file` (`core::line!()`).
    pub line: u32,
    /// Monotonic timestamp in nanoseconds. `0` if unavailable.
    pub timestamp_ns: u64,
}

/// Backend a log record is delivered to.
///
/// Implementations must be `Sync` so the dispatcher can hold them
/// in `&'static [&dyn LogSink]`. ISR-safety is per-impl — see the
/// table in `docs/roadmap/archived/phase-88-nros-log.md`.
pub trait LogSink: Sync {
    /// Render `record`. Called only when the record's severity passes
    /// both the compile-time ceiling AND the [`Logger`]'s runtime
    /// threshold.
    fn log(&self, record: &Record<'_>);

    /// Optional flush hook (default no-op).
    fn flush(&self) {}
}

/// A named logger with a runtime severity threshold.
///
/// Threshold defaults to [`Severity::Info`]. Use [`register_logger`]
/// to publish a `'static Logger` so multiple call sites with the
/// same name share the same threshold.
pub struct Logger {
    name: &'static str,
    level: AtomicU8,
}

impl Logger {
    /// `const`-construct with the default threshold ([`Severity::Info`]).
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            level: AtomicU8::new(Severity::Info as u8),
        }
    }

    /// `const`-construct with an explicit threshold.
    #[must_use]
    pub const fn with_level(name: &'static str, level: Severity) -> Self {
        Self {
            name,
            level: AtomicU8::new(level as u8),
        }
    }

    /// Logger name (used as `Record::logger_name`).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Current runtime threshold.
    #[must_use]
    pub fn level(&self) -> Severity {
        severity_from_u8(self.level.load(Ordering::Relaxed)).unwrap_or(Severity::Info)
    }

    /// Update the runtime threshold.
    pub fn set_level(&self, level: Severity) {
        self.level.store(level as u8, Ordering::Relaxed);
    }

    /// Whether a record at `severity` would be emitted by this
    /// logger AT RUNTIME.
    #[must_use]
    pub fn is_enabled(&self, severity: Severity) -> bool {
        (severity as u8) >= self.level.load(Ordering::Relaxed)
    }

    /// Hand `record` to every registered sink, after the runtime
    /// threshold check.
    ///
    /// Macros call this; user code should not.
    pub fn dispatch(&self, record: &Record<'_>) {
        if !self.is_enabled(record.severity) {
            return;
        }
        dispatch_to_sinks(record);
    }
}

// -----------------------------------------------------------------------------
// Static intern table for `get_logger("name")`. Bounded; no alloc.
// -----------------------------------------------------------------------------

/// Maximum number of named loggers that can be registered via
/// [`register_logger`]. Beyond this, [`get_logger`] returns
/// [`DEFAULT_LOGGER`].
pub const MAX_LOGGERS: usize = 32;

/// Catch-all logger returned when the requested name is not
/// registered (or the intern table is full).
pub static DEFAULT_LOGGER: Logger = Logger::new("nros");

mod intern {
    use super::{AtomicPtr, Logger, MAX_LOGGERS, Ordering};

    pub(super) struct InternTable {
        slots: [AtomicPtr<Logger>; MAX_LOGGERS],
    }

    impl InternTable {
        pub(super) const fn new() -> Self {
            // `AtomicPtr::new` is `const` on both `core::sync::atomic`
            // and `portable_atomic`, so we can initialise the array
            // by repeating the call rather than naming a `const` —
            // which clippy flags as interior-mutable.
            #[allow(clippy::declare_interior_mutable_const)]
            const NULL: AtomicPtr<Logger> = AtomicPtr::new(core::ptr::null_mut());
            Self {
                slots: [NULL; MAX_LOGGERS],
            }
        }

        pub(super) fn lookup(&self, name: &str) -> Option<&'static Logger> {
            for slot in &self.slots {
                let ptr = slot.load(Ordering::Acquire);
                if ptr.is_null() {
                    return None;
                }
                // SAFETY: pointer published via Release after the
                // owner constructed a `'static Logger`. The Acquire
                // load synchronizes.
                let logger: &'static Logger = unsafe { &*ptr };
                if logger.name() == name {
                    return Some(logger);
                }
            }
            None
        }

        pub(super) fn insert(&self, logger: &'static Logger) -> Option<&'static Logger> {
            if let Some(existing) = self.lookup(logger.name()) {
                return Some(existing);
            }
            let ptr = logger as *const _ as *mut Logger;
            for slot in &self.slots {
                if slot
                    .compare_exchange(
                        core::ptr::null_mut(),
                        ptr,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    return Some(logger);
                }
                let existing_ptr = slot.load(Ordering::Acquire);
                if !existing_ptr.is_null() {
                    // SAFETY: same publication invariant as `lookup`.
                    let existing: &'static Logger = unsafe { &*existing_ptr };
                    if existing.name() == logger.name() {
                        return Some(existing);
                    }
                }
            }
            None
        }
    }
}

static INTERN: intern::InternTable = intern::InternTable::new();

/// Publish `logger` under its name so subsequent `get_logger`
/// calls with that name return THIS reference.
///
/// On name collision returns the pre-existing entry (the input
/// `logger` is NOT inserted). On a full table returns
/// [`DEFAULT_LOGGER`].
pub fn register_logger(logger: &'static Logger) -> &'static Logger {
    INTERN.insert(logger).unwrap_or(&DEFAULT_LOGGER)
}

/// Look up a registered logger by name. Returns [`DEFAULT_LOGGER`]
/// if none is registered (call [`register_logger`] for a `'static
/// Logger` to publish one).
///
/// Total — every call returns a usable handle the macros can
/// dispatch through.
#[must_use]
pub fn get_logger(name: &str) -> &'static Logger {
    INTERN.lookup(name).unwrap_or(&DEFAULT_LOGGER)
}

// -----------------------------------------------------------------------------
// Sink list. Set once at `init`; read every dispatch.
// -----------------------------------------------------------------------------

static SINKS_PTR: AtomicPtr<&'static [&'static dyn LogSink]> =
    AtomicPtr::new(core::ptr::null_mut());

/// Install the global sink list.
///
/// MUST be called at app startup BEFORE any record-emitting macro
/// runs (otherwise the dispatch is a no-op — records are silently
/// dropped). Calling `init` more than once swaps the list
/// atomically; the previous pointer is leaked (intentional: the
/// read path is lock-free so we can't safely free).
///
/// The sinks themselves must outlive the program (`'static`).
pub fn init(sinks: &'static [&'static dyn LogSink]) {
    // Indirect through a small `'static` cell so the read path
    // dereferences a fat-pointer-sized slot rather than reading
    // a wide pointer atomically.
    #[cfg(feature = "alloc")]
    {
        let boxed: alloc::boxed::Box<&'static [&'static dyn LogSink]> =
            alloc::boxed::Box::new(sinks);
        let ptr = alloc::boxed::Box::into_raw(boxed);
        SINKS_PTR.store(ptr, Ordering::Release);
    }
    #[cfg(not(feature = "alloc"))]
    {
        static CELL: SinkSlot = SinkSlot::new();
        CELL.store(sinks);
        SINKS_PTR.store(CELL.as_ptr(), Ordering::Release);
    }
}

#[cfg(not(feature = "alloc"))]
struct SinkSlot {
    inner: core::cell::UnsafeCell<Option<&'static [&'static dyn LogSink]>>,
}

#[cfg(not(feature = "alloc"))]
// SAFETY: only written from `init`, which the user contracts to call
// once at startup before any concurrent reader exists.
unsafe impl Sync for SinkSlot {}

#[cfg(not(feature = "alloc"))]
impl SinkSlot {
    const fn new() -> Self {
        Self {
            inner: core::cell::UnsafeCell::new(None),
        }
    }
    fn store(&self, sinks: &'static [&'static dyn LogSink]) {
        // SAFETY: see Sync note above.
        unsafe {
            *self.inner.get() = Some(sinks);
        }
    }
    fn as_ptr(&self) -> *mut &'static [&'static dyn LogSink] {
        self.inner.get().cast()
    }
}

fn dispatch_to_sinks(record: &Record<'_>) {
    if recursion_guard_check_and_set() {
        return;
    }
    let ptr = SINKS_PTR.load(Ordering::Acquire);
    if !ptr.is_null() {
        // SAFETY: `init` published a valid `'static` slice reference.
        let sinks: &'static [&'static dyn LogSink] = unsafe { *ptr };
        for sink in sinks {
            sink.log(record);
        }
    }
    recursion_guard_clear();
}

/// Flush every registered sink.
pub fn flush() {
    let ptr = SINKS_PTR.load(Ordering::Acquire);
    if ptr.is_null() {
        return;
    }
    // SAFETY: same invariant as `dispatch_to_sinks`.
    let sinks: &'static [&'static dyn LogSink] = unsafe { *ptr };
    for sink in sinks {
        sink.flush();
    }
}

// -----------------------------------------------------------------------------
// Recursion guard — process-global single AtomicBool.
//
// Granularity is intentionally coarse (process-wide, not per-thread).
// The guard exists to break a sink that triggers log() during write
// — not to serialize concurrent loggers across threads. A thread
// re-entering through its own sink loses its other in-flight
// sinks for that call; a different thread logging concurrently is
// also short-circuited momentarily. This is acceptable: the alt is
// per-thread storage which doesn't exist uniformly across our
// `no_std` targets (`thread_local!` requires `std`).
// -----------------------------------------------------------------------------

use portable_atomic::AtomicBool;
static RECURSION_GUARD: AtomicBool = AtomicBool::new(false);

fn recursion_guard_check_and_set() -> bool {
    RECURSION_GUARD
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
        .is_err()
}

fn recursion_guard_clear() {
    RECURSION_GUARD.store(false, Ordering::Release);
}

// -----------------------------------------------------------------------------
// Tests (host-only).
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_round_trips_through_u8() {
        for s in [
            Severity::Trace,
            Severity::Debug,
            Severity::Info,
            Severity::Warn,
            Severity::Error,
            Severity::Fatal,
        ] {
            assert_eq!(severity_from_u8(s.as_u8()), Some(s));
        }
        assert_eq!(severity_from_u8(99), None);
    }

    #[test]
    fn logger_runtime_threshold_filters_below() {
        let logger = Logger::with_level("test_thresh", Severity::Warn);
        assert!(!logger.is_enabled(Severity::Info));
        assert!(logger.is_enabled(Severity::Warn));
        assert!(logger.is_enabled(Severity::Error));
        logger.set_level(Severity::Debug);
        assert!(logger.is_enabled(Severity::Info));
    }

    #[test]
    fn unregistered_get_logger_returns_default() {
        let l = get_logger("definitely-not-registered-99");
        assert_eq!(l.name(), DEFAULT_LOGGER.name());
    }

    #[test]
    fn registered_logger_round_trips_through_intern_table() {
        static LOGGER: Logger = Logger::new("test_intern_round_trip");
        let published = register_logger(&LOGGER);
        assert_eq!(published.name(), LOGGER.name());
        let looked_up = get_logger("test_intern_round_trip");
        assert!(core::ptr::eq(published, looked_up));
    }

    #[test]
    fn compile_time_ceiling_matches_enabled_feature() {
        let expected = if cfg!(feature = "max-level-off") {
            None
        } else if cfg!(feature = "max-level-trace") {
            Some(Severity::Trace)
        } else if cfg!(feature = "max-level-debug") {
            Some(Severity::Debug)
        } else if cfg!(feature = "max-level-info") {
            Some(Severity::Info)
        } else if cfg!(feature = "max-level-warn") {
            Some(Severity::Warn)
        } else if cfg!(feature = "max-level-error") {
            Some(Severity::Error)
        } else {
            // No ceiling feature = treat as `max-level-trace`.
            Some(Severity::Trace)
        };

        for severity in [
            Severity::Trace,
            Severity::Debug,
            Severity::Info,
            Severity::Warn,
            Severity::Error,
            Severity::Fatal,
        ] {
            let enabled = expected.is_some_and(|ceiling| severity >= ceiling);
            assert_eq!(severity_enabled_at_compile_time(severity), enabled);
        }
    }
}
