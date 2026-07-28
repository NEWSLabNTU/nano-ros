//! ThreadX Linux nros-log smoke fixture.
//!
//! Boots via the board crate so `run()` registers the ThreadX log
//! writer, then emits every severity through `nros-log`.

use nros_board_threadx_linux::ThreadxLinux;
use nros_log::{
    Logger, Severity, init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn,
    register_logger, sinks,
};

static LOGGER: Logger = Logger::new("smoke");

fn main() {
    // Phase 313 W0 (#0243) — boot via the new-family NO-SESSION `run_bare`
    // (was the legacy `run(Config, closure)`). A logging fixture opens no ROS
    // session, so `run_bare` boots the kernel + log writer and runs this closure
    // directly — `BoardEntry::run` would abort on `Executor::open` with no router.
    let _ = ThreadxLinux::run_bare(|| {
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
    });
}
