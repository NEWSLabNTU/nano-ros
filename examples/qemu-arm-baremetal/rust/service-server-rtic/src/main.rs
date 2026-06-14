//! RTIC AddTwoInts service-server entry for nros on QEMU MPS2-AN385 (phase-244.D1).
//!
//! Collapses to `nros::main!()`: the macro reads
//! `[package.metadata.nros.entry] deploy = "rtic-mps2-an385"`, resolves the RTIC
//! board (`nros-board-rtic-mps2-an385`), and emits the `#[rtic::app]` boot
//! scaffold that brings up hardware/network, opens the executor, registers the
//! linked RMW, and runs the `service_server_rtic_pkg` node's `register` + RTIC
//! dispatch loop. Locator/domain come from the deploy overlay — no hardcoded
//! `Config`, no manual `Executor::open`, no hand-written `handle_request` loop.

#![no_std]
#![no_main]

use panic_semihosting as _;

nros::main!();
