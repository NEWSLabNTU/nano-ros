//! Phase 244.D1 — bare-metal `nros::main!()` BoardEntry E2E entry pkg.
//!
//! The whole boot scaffold (reset vector via `#[cortex_m_rt::entry]`, RMW
//! register, `Executor::open`, `RuntimeCtx`, node registration, spin) is
//! emitted by `nros::main!()` from `[package.metadata.nros.entry] deploy =
//! "qemu-mps2-an385"` + `node_pkgs`. The only hand-written lines are the
//! bare-metal attrs + panic handler.

#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!();
