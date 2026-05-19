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

use crate::{LogSink, Record};

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
/// `docs/roadmap/phase-88-nros-log.md`.
pub struct PlatformSink;

impl LogSink for PlatformSink {
    fn log(&self, record: &Record<'_>) {
        let name = record.logger_name.as_bytes();
        let msg = record.message.as_bytes();
        // SAFETY: pointers come from `&str` / `&'a [u8]` references
        // that outlive the call; lengths match.
        unsafe {
            nros_platform_log_write(
                record.severity.as_u8(),
                name.as_ptr(),
                name.len(),
                msg.as_ptr(),
                msg.len(),
            );
        }
    }

    fn flush(&self) {
        // SAFETY: no args, no preconditions.
        unsafe { nros_platform_log_flush() };
    }
}

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
    static SINKS: &[&dyn LogSink] = &[&PLATFORM_SINK];
    SINKS
}
