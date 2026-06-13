//! Bare-metal `nros::main!()` BoardEntry listener for QEMU MPS2-AN385.
//!
//! The whole boot scaffold (reset vector via `#[cortex_m_rt::entry]`, RMW
//! register, `Executor::open`, `RuntimeCtx`, node registration, spin) is
//! emitted by `nros::main!()` from `[package.metadata.nros.entry] deploy =
//! "qemu-mps2-an385"`. The only hand-written lines are the bare-metal attrs +
//! panic handler. The application logic lives in the sibling `listener_pkg`
//! Node pkg (re-exported via src/lib.rs).

#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!();
