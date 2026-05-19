//! Phase 88.15.a — bare-metal MPS2-AN385 nros-log smoke fixture.
//!
//! Emits exactly one record per [`Severity`], drives them through
//! `nros-log` → `PlatformSink` → `nros_platform_log_write` →
//! `nros-platform-mps2-an385`'s semihosting writer, then exits with
//! `EXIT_SUCCESS` so the host harness can assert on the captured
//! semihosting output.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::debug;
use nros_log::{
    init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn, register_logger,
    sinks, Logger, Severity,
};
use panic_semihosting as _;

// Force-link the per-platform PlatformLog impl so its
// `nros_platform_log_write` symbol resolves at link time. Without this
// the optimizer can drop the `nros-platform-mps2-an385` rlib and the
// PlatformSink call hits a NULL slot.
extern crate nros_platform_mps2_an385 as _;

static LOGGER: Logger = Logger::new("smoke");

#[entry]
fn main() -> ! {
    init(sinks::default());
    let logger = register_logger(&LOGGER);
    // Drop the threshold so every severity macro emits.
    logger.set_level(Severity::Trace);

    nros_trace!(logger, "trace payload");
    nros_debug!(logger, "debug payload");
    nros_info!(logger, "info payload");
    nros_warn!(logger, "warn payload");
    nros_error!(logger, "error payload");
    nros_fatal!(logger, "fatal payload");

    nros_log::flush();
    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}
