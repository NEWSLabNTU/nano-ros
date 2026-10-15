//! Issue 1906 — the platform condvar contract, run on FreeRTOS + MPS2-AN385.
//!
//! Boots FreeRTOS with no session and runs `nros_condvar_contract::run` from
//! the app task, printing one line per check through `nros-log` (semihosting),
//! then exits QEMU with the result. FreeRTOS keeps a mutex's recursion count
//! private, so the issue-1907 "held twice" refusal is not checked here. It
//! also checks zenoh-pico's vendored `_z_condvar_wait` (phase-502 W2).

#![no_std]
#![no_main]

use condvar_contract_freertos_mps2_nros_selection::nros_board_mps2_an385_freertos::{
    Config, Mps2An385,
};
use cortex_m_semihosting::debug::{EXIT_FAILURE, EXIT_SUCCESS, exit};
use nros_condvar_contract::{ALL_PASSED, FAIL_PREFIX, PASS_PREFIX};
use nros_log::{Logger, Severity, log_error, log_info, register_logger};
use panic_semihosting as _;

extern crate nros_platform as _;

static LOGGER: Logger = Logger::new("condvar");

const CONFIG: &str = include_str!("../config.toml");

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = Mps2An385::run_bare(Config::from_toml(CONFIG), |_config| {
        register_logger(&LOGGER);
        LOGGER.set_level(Severity::Info);
        let mut report = |name: &str, ok: bool, detail: &str| {
            if ok {
                log_info!(&LOGGER, "{}{}: {}", PASS_PREFIX, name, detail);
            } else {
                log_error!(&LOGGER, "{}{}: {}", FAIL_PREFIX, name, detail);
            }
        };
        let mut passed = nros_condvar_contract::run(&mut report, false);
        // Phase-502 W2: zenoh-pico's own `_z_condvar_wait` is bounded.
        passed &= nros_condvar_contract::run_zenoh_wait(&mut report);
        passed &= nros_condvar_contract::run_zenoh_wait_until_wrapped(&mut report);
        if passed {
            log_info!(&LOGGER, "{}", ALL_PASSED);
        }
        nros_log::flush();
        exit(if passed { EXIT_SUCCESS } else { EXIT_FAILURE });

        #[allow(unreachable_code)]
        Ok::<(), &'static str>(())
    });
    unreachable!()
}
