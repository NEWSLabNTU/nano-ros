//! Phase 88.15.b — FreeRTOS + MPS2-AN385 nros-log smoke fixture.
//!
//! Boots FreeRTOS via the board crate's `run()`, lets the board
//! crate register its semihosting log writer (Phase 88.11), then
//! emits one record per [`nros_log::Severity`] from the app task
//! and exits via semihosting `EXIT_SUCCESS`. The companion harness
//! (`packages/testing/nros-tests/tests/logging_smoke.rs`) drains
//! the QEMU semihosting stderr and asserts every `[<LEVEL>] smoke: …`
//! line appears.

#![no_std]
#![no_main]

use nros_board_mps2_an385_freertos::{Config, run};
use nros_log::{
    init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn, register_logger,
    sinks, Logger, Severity,
};
use panic_semihosting as _;

// Link `nros-platform` so its FreeRTOS C symbols + `global-allocator`
// adapter end up in the binary even though we do not name them.
extern crate nros_platform as _;

static LOGGER: Logger = Logger::new("smoke");

// Network config lives in a sibling `config.toml`, compile-baked here
// (RFC-0004: config in a file, not hardcoded in code). `from_toml` applies the
// build-time `NROS_DOMAIN_ID` override for per-fixture domain isolation.
const CONFIG: &str = include_str!("../config.toml");

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    run(Config::from_toml(CONFIG), |_config| {
        register_logger(&LOGGER);
        init(sinks::default());
        let logger = &LOGGER;
        logger.set_level(Severity::Trace);

        nros_trace!(logger, "trace payload");
        nros_debug!(logger, "debug payload");
        nros_info!(logger, "info payload");
        nros_warn!(logger, "warn payload");
        nros_error!(logger, "error payload");
        nros_fatal!(logger, "fatal payload");
        nros_log::flush();

        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);

        #[allow(unreachable_code)]
        Ok::<(), &'static str>(())
    })
}
