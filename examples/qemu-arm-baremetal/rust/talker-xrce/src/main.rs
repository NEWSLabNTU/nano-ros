//! XRCE (custom UART transport) talker entry for QEMU MPS2-AN385 (phase-244.D1).
//!
//! Collapses to `nros::main!()`: the macro reads
//! `[package.metadata.nros.entry] deploy = "qemu-mps2-an385"`, resolves the
//! bare-metal board, and emits the `#[cortex_m_rt::entry]` boot scaffold. The
//! `[…deploy.qemu-mps2-an385] transport = "xrce"` overlay makes the macro call
//! `BoardEntry::setup_transport` (the board, built `xrce-transport`, installs the
//! XRCE-over-UART vtable) BEFORE `__register_linked_rmw()` registers the XRCE
//! backend — the ordering `set_custom_transport_ops` needs. Node logic lives in
//! `xrce_talker_pkg`. No hand-written `xrce::set_custom_transport_ops` /
//! `xrce::register` / `Executor::open` ceremony.

#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!();
