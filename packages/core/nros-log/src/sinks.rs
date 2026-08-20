//! Phase 88.4 — `PlatformSink` + helpers.
//!
//! The portable facade ships exactly ONE sink: [`PlatformSink`].
//! It forwards every record to `nros_platform_log_write` (declared
//! in `nros-platform-cffi` / extended in Phase 88.3). Per-platform
//! `nros-platform-<rtos>` crates own the actual delivery.
//!
//! Apps wanting fan-out (e.g. `Platform + /rosout` or stdout in a
//! test harness) compose their own `&'static [&dyn LogSink]` and
//! pass it to [`crate::init`].

use crate::LogSink;
#[cfg(feature = "platform-sink")]
use crate::Record;

#[cfg(feature = "platform-sink")]
unsafe extern "C" {
    /// Per-platform log delivery (Phase 88). Declared in
    /// `<nros/platform.h>`; implementor lives in each
    /// `nros-platform-<rtos>` crate.
    pub fn nros_platform_log_write(
        severity: u8,
        name_ptr: *const u8,
        name_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    );

    /// Per-platform log flush. Default no-op on platforms that
    /// don't buffer.
    pub fn nros_platform_log_flush();
}

/// The default sink: forwards to `nros_platform_log_write`.
///
/// Zero-sized. Threading + ISR safety inherit from the linked
/// `nros-platform-<rtos>` impl — see the table in
/// `docs/roadmap/archived/phase-88-nros-log.md`.
#[cfg(feature = "platform-sink")]
pub struct PlatformSink;

#[cfg(feature = "platform-sink")]
impl LogSink for PlatformSink {
    fn log(&self, record: &Record<'_>) {
        // Issue #503 — with `platform-clock`, prefix the rendered line
        // with the record's monotonic stamp as `[sssss.uuuuuu]`. Done
        // by message rewrite because the `nros_platform_log_write` ABI
        // has no timestamp parameter and widening it would touch every
        // platform port; the prefix is additive, not a re-format of
        // the caller's text. A record without a stamp (`0`) passes
        // through untouched.
        #[cfg(feature = "platform-clock")]
        if record.timestamp_ns != 0 {
            use core::fmt::Write as _;
            let secs = record.timestamp_ns / 1_000_000_000;
            let micros = (record.timestamp_ns % 1_000_000_000) / 1_000;
            let mut buf = crate::FormatBuffer::new();
            let _ = core::write!(buf, "[{secs:5}.{micros:06}] {}", record.message);
            emit(record.severity.as_u8(), record.logger_name, buf.as_str());
            return;
        }
        emit(record.severity.as_u8(), record.logger_name, record.message);
    }

    fn flush(&self) {
        // SAFETY: no args, no preconditions.
        unsafe { nros_platform_log_flush() };
    }
}

#[cfg(feature = "platform-sink")]
fn emit(severity: u8, name: &str, msg: &str) {
    let name = name.as_bytes();
    let msg = msg.as_bytes();
    // SAFETY: pointers come from `&str` references that outlive the
    // call; lengths match.
    unsafe {
        nros_platform_log_write(severity, name.as_ptr(), name.len(), msg.as_ptr(), msg.len());
    }
}

#[cfg(feature = "platform-sink")]
static PLATFORM_SINK: PlatformSink = PlatformSink;

/// The default sink list: just `&PLATFORM_SINK`.
///
/// Pass to [`crate::init`] for the common case:
///
/// ```ignore
/// nros_log::init(nros_log::sinks::default());
/// ```
#[must_use]
pub fn default() -> &'static [&'static dyn LogSink] {
    #[cfg(feature = "platform-sink")]
    {
        static SINKS: &[&dyn LogSink] = &[&PLATFORM_SINK];
        SINKS
    }
    // Without the platform sink there is nothing to deliver to — an empty
    // list, records drop. Only host test lanes build this shape; every real
    // image carries the default feature set.
    #[cfg(not(feature = "platform-sink"))]
    {
        &[]
    }
}
