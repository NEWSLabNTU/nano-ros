//! Phase 88.12 — C-API surface for the `nros_log` facade.
//!
//! Mirrors `<nros/log.h>`. The `nros_log_emit` /
//! `nros_log_emit_fmt` symbols dispatch through the same per-platform
//! sink chain the Rust call sites use (Phase 88.5 onwards).
//!
//! cbindgen is told to skip every item in this module — the
//! hand-written `<nros/log.h>` is authoritative for the C ABI
//! (cbindgen would re-emit the enum + functions under their
//! mangled names, colliding with the hand-written header).

use core::ffi::{c_char, c_void};

/// C severity mirror of `nros_log::Severity`. Discriminants match
/// `Severity::as_u8()`.
///
/// cbindgen:ignore
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum nros_log_severity_t {
    NROS_LOG_SEVERITY_TRACE = 0,
    NROS_LOG_SEVERITY_DEBUG = 1,
    NROS_LOG_SEVERITY_INFO = 2,
    NROS_LOG_SEVERITY_WARN = 3,
    NROS_LOG_SEVERITY_ERROR = 4,
    NROS_LOG_SEVERITY_FATAL = 5,
}

impl nros_log_severity_t {
    fn to_facade(self) -> nros_log::Severity {
        match self {
            Self::NROS_LOG_SEVERITY_TRACE => nros_log::Severity::Trace,
            Self::NROS_LOG_SEVERITY_DEBUG => nros_log::Severity::Debug,
            Self::NROS_LOG_SEVERITY_INFO => nros_log::Severity::Info,
            Self::NROS_LOG_SEVERITY_WARN => nros_log::Severity::Warn,
            Self::NROS_LOG_SEVERITY_ERROR => nros_log::Severity::Error,
            Self::NROS_LOG_SEVERITY_FATAL => nros_log::Severity::Fatal,
        }
    }
}

/// Low-level emit. `message` is UTF-8 text + explicit length; the
/// dispatcher hands it to whichever sink list was registered via
/// `nros_log::init`.
///
/// `logger` is the opaque handle from `nros_node_get_logger(...)`;
/// passing NULL drops the record silently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_log_emit(
    logger: *const c_void,
    severity: nros_log_severity_t,
    message: *const c_char,
    message_len: usize,
) {
    if logger.is_null() {
        return;
    }
    // Lazy-install the default sink list on first emit so C/C++
    // call sites work without an explicit `nros_log_init` step from
    // the user. Rust callers that want a custom sink list can still
    // call `nros_log::init(...)` before any record fires (the
    // install is idempotent — replacing the pointer is fine).
    ensure_default_sinks();
    let logger: &'static nros_log::Logger = &*(logger as *const nros_log::Logger);
    let sev = severity.to_facade();
    if !logger.is_enabled(sev) {
        return;
    }
    let msg_bytes: &[u8] = if message.is_null() || message_len == 0 {
        &[]
    } else {
        core::slice::from_raw_parts(message as *const u8, message_len)
    };
    let msg_str = core::str::from_utf8(msg_bytes).unwrap_or("<invalid utf-8>");
    let record = nros_log::Record {
        severity: sev,
        logger_name: logger.name(),
        message: msg_str,
        file: "<nros-c>",
        line: 0,
        timestamp_ns: 0,
    };
    logger.dispatch(&record);
}

use core::sync::atomic::{AtomicBool, Ordering};
static DEFAULT_SINKS_INSTALLED: AtomicBool = AtomicBool::new(false);

fn ensure_default_sinks() {
    if DEFAULT_SINKS_INSTALLED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        nros_log::init(nros_log::sinks::default());
    }
}

// `nros_log_emit_fmt` is implemented in C
// (`packages/core/nros-c/c-stubs/log_fmt.c`) because the Rust
// `c_variadic` feature is still unstable on stable. The C shim
// vsnprintfs the format args + forwards to `nros_log_emit` above.
unsafe extern "C" {
    pub fn nros_log_emit_fmt(
        logger: *const c_void,
        severity: nros_log_severity_t,
        fmt: *const c_char,
        ...
    );
}

// Force the Rust symbol to land even when the linker greedily prunes.
#[used]
static _ANCHOR: unsafe extern "C" fn(*const c_void, nros_log_severity_t, *const c_char, usize) =
    nros_log_emit;
