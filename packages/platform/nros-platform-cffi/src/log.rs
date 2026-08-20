//! The `LogSink` that speaks the platform ABI.
//!
//! ## Why it lives here and not in the facade
//!
//! `nros-log` is a facade: `LogSink` + `init` exist so delivery is PLUGGABLE.
//! `PlatformSink` is one bridge among possible many — it is the only one that
//! needs `nros_platform_log_write`, which is a LINK-TIME requirement on the
//! final binary.
//!
//! While it lived in `nros-log`, "does this binary need the platform log ABI?"
//! was answerable only by a Cargo feature, and issue 0710 showed that cannot
//! work: `nros-platform-cffi` and `nros-rmw-bridge` enable
//! `nros-node/rmw-cffi` unconditionally, so workspace feature unification
//! turns any forwarded gate back ON for every member of the build. A feature is
//! a property of the BUILD.
//!
//! A DEPENDENCY is a property of the binary. Here, the question answers itself:
//! a binary that links this crate has the ABI (that is what this crate is), and
//! one that does not cannot accidentally acquire the requirement. The extern is
//! also declared exactly once now, in `generated.rs` — bindgen output from
//! `<nros/platform.h>`, the SSoT per RFC-0054 — rather than a second time by
//! hand in the facade.
//!
//! ## Using it
//!
//! ```ignore
//! nros_log::init(nros_platform_cffi::log::default_sinks());
//! // or, equivalently:
//! nros_platform_cffi::log::init_default();
//! ```
//!
//! Apps wanting fan-out (e.g. platform + `/rosout`) compose their own
//! `&'static [&dyn LogSink]` and pass it to `nros_log::init`, exactly as before.

use nros_log::{FormatBuffer, LogSink, Record};

/// Forwards every record to the platform port's `nros_platform_log_write`.
///
/// Zero-sized. Threading + ISR safety inherit from the linked
/// `nros-platform-<rtos>` impl — see the table in
/// `docs/roadmap/archived/phase-88-nros-log.md`.
pub struct PlatformSink;

impl LogSink for PlatformSink {
    fn log(&self, record: &Record<'_>) {
        // Issue #503 — prefix the rendered line with the record's monotonic
        // stamp as `[sssss.uuuuuu]`. Done by message rewrite because the
        // `nros_platform_log_write` ABI has no timestamp parameter and widening
        // it would touch every platform port; the prefix is additive, not a
        // re-format of the caller's text.
        //
        // Keyed on the STAMP, not on a Cargo feature. In `nros-log` this was
        // `#[cfg(feature = "platform-clock")]`, which cannot follow the sink
        // across a crate boundary without inventing a second feature that means
        // the same thing. A record with no stamp (`0`) is already the "no clock"
        // case, so the runtime check subsumes the cfg — and one branch on a u64
        // is not what a log path is spending its time on.
        if record.timestamp_ns != 0 {
            use core::fmt::Write as _;
            let secs = record.timestamp_ns / 1_000_000_000;
            let micros = (record.timestamp_ns % 1_000_000_000) / 1_000;
            let mut buf = FormatBuffer::new();
            let _ = core::write!(buf, "[{secs:5}.{micros:06}] {}", record.message);
            emit(record.severity.as_u8(), record.logger_name, buf.as_str());
            return;
        }
        emit(record.severity.as_u8(), record.logger_name, record.message);
    }

    fn flush(&self) {
        // SAFETY: no args, no preconditions.
        unsafe { crate::generated::nros_platform_log_flush() };
    }
}

fn emit(severity: u8, name: &str, msg: &str) {
    let name = name.as_bytes();
    let msg = msg.as_bytes();
    // SAFETY: pointers come from `&str` references that outlive the call;
    // lengths match.
    unsafe {
        crate::generated::nros_platform_log_write(
            severity,
            name.as_ptr(),
            name.len(),
            msg.as_ptr(),
            msg.len(),
        );
    }
}

static PLATFORM_SINK: PlatformSink = PlatformSink;

/// The default sink list: just [`PlatformSink`].
#[must_use]
pub fn default_sinks() -> &'static [&'static dyn LogSink] {
    static SINKS: &[&dyn LogSink] = &[&PLATFORM_SINK];
    SINKS
}

/// Install [`default_sinks`] as the global sink list.
///
/// Convenience for the common boot funnel; equivalent to
/// `nros_log::init(default_sinks())`, and it drains anything
/// `nros_log::early` held before this point.
pub fn init_default() {
    nros_log::init(default_sinks());
}
