//! Embassy talker on STM32F4 — Phase 216.C.4 `nros::main!()` shape.
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
//! ## Skeleton status
//!
//! `init_hardware`'s body is still `todo!()` (216.C.2 follow-up
//! mirrors the legacy Pattern A bringup), and the trampoline-
//! registration story that hands the sibling `stm32f4_talker_pkg`
//! Node onto the dispatch runtime is the next 216.C wave. The macro
//! emit + dep graph compile clean today; a real flash will hit the
//! `todo!()` panic in `init_hardware`.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
