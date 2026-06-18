//! Phase 88.15.d — ThreadX RISC-V QEMU nros-log smoke fixture.
//!
//! Boots ThreadX via the board crate's `run()` so the UART writer
//! gets wired into `nros-platform-threadx`'s log fn-ptr slot
//! (Phase 88.11), then drives every severity through `nros-log`
//! from the app thread and exits via the QEMU `test-finisher` MMIO
//! device.

#![no_std]
#![no_main]

use nros_board_threadx_qemu_riscv64::{exit_success, Config, run};
use nros_log::{
    init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn, register_logger,
    sinks, Logger, Severity,
};

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

        exit_success();

        #[allow(unreachable_code)]
        Ok::<(), &'static str>(())
    })
}

// Panic handler ships with the board crate — no local definition.
