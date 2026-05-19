//! Phase 88.15.c — NuttX QEMU ARM nros-log smoke fixture.
//!
//! Boots NuttX via the board crate's `run()` (which drives
//! `init_hardware` + the standard `nsh_main` chain), then drives
//! every severity through `nros-log`. The NuttX C platform port
//! (`nros-platform-posix`, shared with NuttX via the
//! `nros-platform-nuttx` shim) renders each record on stderr as
//! `[<LEVEL>] <name>: <message>\n`. Cleanly exits via the closure
//! returning Ok — the board crate's `run()` then calls
//! `std::process::exit(0)`.

use nros_board_nuttx_qemu_arm::{run, Config};
use nros_log::{
    init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn, register_logger,
    sinks, Logger, Severity,
};

static LOGGER: Logger = Logger::new("smoke");

fn main() {
    run(Config::default(), |_config| {
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

        Ok::<(), &'static str>(())
    })
}
