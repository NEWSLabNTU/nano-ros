#![no_std]
#![no_main]
use panic_semihosting as _;
#[unsafe(no_mangle)]
extern "C" fn _start() -> ! { freertos_qemu_mps2_an385_bsp::nros_run() }
