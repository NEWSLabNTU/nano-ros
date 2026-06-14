//! Serial (UART) listener entry for QEMU MPS2-AN385 (phase-244.D1).
//!
//! Collapses to `nros::main!()`: the macro reads
//! `[package.metadata.nros.entry] deploy = "qemu-mps2-an385"`, resolves the
//! bare-metal board, and emits the `#[cortex_m_rt::entry]` boot scaffold that
//! brings up the UART link, opens the executor, registers the linked RMW, and
//! runs `serial_listener_pkg`'s node. The board is built with the `serial`
//! feature so `run_with_deploy` boots `Config::serial_default`; the deploy
//! overlay supplies the `serial/UART_0#…` locator. No hand-written
//! `run(Config::serial_default(), …)` closure, no manual `Executor::open`.

#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!();
