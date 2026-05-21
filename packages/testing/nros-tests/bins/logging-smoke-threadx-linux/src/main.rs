//! ThreadX Linux nros-log smoke fixture.
//!
//! Boots via the board crate so `run()` registers the ThreadX log
//! writer, then emits every severity through `nros-log`.

use nros_board_threadx_linux::{Config, run};
use nros_log::{
    Logger, Severity, init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn,
    register_logger, sinks,
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
