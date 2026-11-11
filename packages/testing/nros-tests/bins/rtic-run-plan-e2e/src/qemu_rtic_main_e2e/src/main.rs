//! Phase 216.B — the RTIC run-plan E2E entry. `nros::main!()` emits the RTIC
//! app from the bringup it names (`qemu_rtic_main_e2e_bringup`, whose
//! `[image.mps2-an385-baremetal]` names this package as its OWNED entry,
//! RFC-0071 D19).
#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!(launch = "qemu_rtic_main_e2e_bringup", panic = "own");
