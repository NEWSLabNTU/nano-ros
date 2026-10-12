//! RTIC AddTwoInts service-client entry for nros on QEMU MPS2-AN385 (phase-244.D1).
//!
//! Collapses to `nros::main!()`: the macro reads
//! `[image.*] board = "rtic-mps2-an385"`, resolves the RTIC
//! board (`nros-board-mps2-an385`), and emits the `#[rtic::app]` boot
//! scaffold that brings up hardware/network, opens the executor, registers the
//! linked RMW, and runs the `service_client_rtic_pkg` node's `register` + RTIC
//! dispatch loop. Locator/domain come from the deploy overlay — no hardcoded
//! `Config`, no manual `Executor::open`, no hand-written service-call loop.

#![no_std]
#![no_main]

// The panic handler is the board's crate-root item (`[board.entry]
// crate_root_extra`), carried by the generated selection package (issue 1305).

nros::main!(panic = "own");
