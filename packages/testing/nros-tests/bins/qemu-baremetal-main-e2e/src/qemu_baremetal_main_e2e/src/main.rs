//! Phase 244.D1 — bare-metal `nros::main!()` BoardEntry E2E entry pkg.
//!
//! The whole boot scaffold (reset vector via `#[cortex_m_rt::entry]`, RMW
//! register, `Executor::open`, `RuntimeCtx`, node registration, spin) is
//! emitted by `nros::main!()` from the bringup it names
//! (`qemu_baremetal_main_e2e_bringup`, whose `[image.mps2-an385-baremetal]`
//! names this package as its OWNED entry, RFC-0071 D19). The only
//! hand-written lines are the bare-metal attrs and the panic policy.

#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!(launch = "qemu_baremetal_main_e2e_bringup", panic = "own");
