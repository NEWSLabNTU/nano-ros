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
//! ## Skeleton status
//!
//! `init_hardware`'s body is still `todo!()` (216.C.2 follow-up
//! mirrors the legacy Pattern A bringup), and the trampoline-
//! registration story that hands the sibling `stm32f4_listener_pkg`
//! Node onto the dispatch runtime — including threading the
//! `embassy_executor::Spawner` through to `ListenerState::spawner`
//! — is the next 216.C wave. The macro emit + dep graph compile
//! clean today; a real flash will hit the `todo!()` panic in
//! `init_hardware`.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
