//! Phase 88.15.c — NuttX QEMU ARM nros-log smoke fixture.
//!
//! Boots NuttX through the board crate's `nsh_main` override, then
//! drives every severity through `nros-log`. The NuttX C platform path
//! routes records through syslog so the QEMU harness can assert the
//! captured UART output.

use core::ffi::c_char;

use nros_log::{
    Logger, Severity, init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn,
    register_logger, sinks,
};

static LOGGER: Logger = Logger::new("smoke");

fn emit_logs() {
    register_logger(&LOGGER);
    init(sinks::default());
    LOGGER.set_level(Severity::Trace);

    nros_trace!(&LOGGER, "trace payload");
    nros_debug!(&LOGGER, "debug payload");
    nros_info!(&LOGGER, "info payload");
    nros_warn!(&LOGGER, "warn payload");
    nros_error!(&LOGGER, "error payload");
    nros_fatal!(&LOGGER, "fatal payload");
    nros_log::flush();
}

#[unsafe(no_mangle)]
pub extern "C" fn nsh_main(_argc: i32, _argv: *const *const c_char) -> i32 {
    emit_logs();
    0
}

fn main() {
    emit_logs();
}
