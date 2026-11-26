//! phase-578 W2 (RFC-0118 D9) — a fault on FreeRTOS + MPS2-AN385 is reported
//! as the board's `nros-fault:` line, and `nros run` looks its addresses up in
//! this image's ELF.
//!
//! Boots FreeRTOS with no session and, from the app task, calls
//! [`inject_fault`], which executes an undefined instruction. The core takes a
//! UsageFault, which escalates to HardFault (no UsageFault handler is enabled),
//! with the faulting instruction's address stacked as `pc`, so the board's
//! handler reports exactly that line. If the app task gets past the call,
//! nothing faulted.

#![no_std]
#![no_main]

use cortex_m_semihosting::{
    debug::{EXIT_FAILURE, exit},
    hprintln,
};
use nros_board_mps2_an385_freertos::{Config, Mps2An385};
use panic_semihosting as _;

extern crate nros_platform as _;

const CONFIG: &str = include_str!("../config.toml");

/// The fault. The test finds this function's `udf` line in this file and
/// expects the CLI to name it.
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn inject_fault() {
    // SAFETY: deliberately undefined: the point is the fault it raises.
    unsafe { core::arch::asm!("udf #0") }; // fault-report: the faulting line
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = Mps2An385::run_bare(Config::from_toml(CONFIG), |_config| {
        hprintln!("fault-report: injecting a fault");
        inject_fault();
        hprintln!("fault-report: FAIL: the fault was not taken");
        exit(EXIT_FAILURE);

        #[allow(unreachable_code)]
        Ok::<(), &'static str>(())
    });
    unreachable!()
}
