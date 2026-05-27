//! Phase 88.15.f — ESP32-C3 QEMU nros-log smoke fixture.
//!
//! Boots ESP32-C3 + OpenETH via the board crate's `run()`, which
//! registers an `esp_println`-backed writer against the
//! `nros-platform-esp32-qemu` log fn-ptr slot (Phase 88.15.f
//! groundwork). The closure drives every Severity through the
//! `nros-log` facade and then loops forever — QEMU is killed by
//! the harness once the captured output contains every expected
//! line.

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros_board_esp32_qemu::{entry, run, Config};
use nros_log::{
    init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn, register_logger,
    sinks, Logger, Severity,
};

nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

static LOGGER: Logger = Logger::new("smoke");

const CONFIG: &str = "\
[node]\n\
domain_id = 0\n\
\n\
[[transport]]\n\
kind = \"ethernet\"\n\
ip = \"10.0.2.50/24\"\n\
mac = \"02:00:00:00:00:99\"\n\
gateway = \"10.0.2.2\"\n\
locator = \"tcp/10.0.2.2:7454\"\n\
";

#[entry]
fn main() -> ! {
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

        Ok::<(), &'static str>(())
    })
}
