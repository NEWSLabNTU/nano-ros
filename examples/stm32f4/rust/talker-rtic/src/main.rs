//! RTIC talker on STM32F4 — Phase 216.B.5 `nros::main!()` shape.
//!
//! The body collapses to the one-line macro invocation. `nros::main!()`
//! reads `[package.metadata.nros.entry] deploy = "rtic-stm32f4"`
//! from `Cargo.toml`, routes through the RTIC framework branch
//! (Phase 216.B.3), and emits the `#[rtic::app] mod app` body +
//! dispatch / spin RTIC task sidekicks. The board crate
//! (`nros-board-rtic-stm32f4`, Phase 216.B.2) supplies the
//! `RticStm32F4` ZST whose `RticBoardEntry::init_hardware` brings up
//! the hardware and returns the `(Executor, RticRuntime)` pair.
//!
//! ## Runtime status (issue #221 doc refresh, 2026-07-17)
//!
//! The old "skeleton" caveats are gone: `RticBoardEntry::init_hardware`
//! does the full bringup (clocks / RMII / smoltcp / zenoh register — it
//! delegates to `nros_board_stm32f4::init_hardware`), and phase-289
//! (`c2227f527`, #178) filled the run task, so the RTIC runtime actually
//! DELIVERS — proven end-to-end by the QEMU mps2 RTIC pubsub lane, which
//! shares this entry scaffold. Flashing a NUCLEO-F429ZI boots and runs;
//! on-hardware runtime validation on a physical bench is the remaining
//! (hardware-gated) tail.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
