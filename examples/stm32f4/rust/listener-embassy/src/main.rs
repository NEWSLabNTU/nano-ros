//! Embassy listener on STM32F4 — Phase 216.C.5 `nros::main!()` shape.
//!
//! The body collapses to the one-line macro invocation. `nros::main!()`
//! reads `[package.metadata.nros.entry] deploy = "embassy-stm32f4"`
//! from `Cargo.toml`, routes through the Embassy framework branch
//! (Phase 216.C.3), and emits the `#[embassy_executor::main] async fn
//! main(spawner)` body + dispatch / spin `#[embassy_executor::task]`
//! sidekicks. The board crate (`nros-board-embassy-stm32f4`, Phase
//! 216.C.2) supplies the `EmbassyStm32F4` ZST whose
//! `EmbassyBoardEntry::init_hardware` brings up the hardware and
//! returns the `(Executor, EmbassyRuntime)` pair.
//!
//! ## Deferred dispatch + spawn-from-sync escape
//!
//! Unlike the sibling `talker-embassy` (which links `talker_pkg`,
//! `DispatchStrategy::Inline`), this Entry pkg links
//! `stm32f4_listener_pkg` whose Node declares
//! `DispatchStrategy::Deferred`. The Embassy board's
//! `NodeDispatchRuntime` enqueues signaled callbacks onto a
//! framework-owned task; the listener's `on_callback` body then
//! enqueues `async` work via the spawn-from-sync escape
//! (`state.spawner.spawn(handle_downstream(msg))`). See
//! `examples/stm32f4/rust/listener_pkg/src/lib.rs` for the body +
//! the Spawner-plumbing TODO.
//!
//! ## Runtime status (issue #221 doc refresh, 2026-07-17)
//!
//! The old "skeleton" caveats are gone: `EmbassyBoardEntry::init_hardware`
//! is implemented (delegates the hardware bringup to the shared
//! `nros_board_stm32f4` path and registers the zenoh backend explicitly —
//! bare-metal `.init_array` doesn't run). Flashing does NOT hit a
//! `todo!()` panic. Caveat, stated honestly: unlike the RTIC siblings
//! (whose runtime is proven by the four QEMU mps2 RTIC e2e lanes,
//! phase-289), the Embassy runtime has no e2e lane yet — the build-stage
//! `embassy_main_macro` cargo-check is the only automated proof, and
//! on-hardware validation is hardware-gated.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
