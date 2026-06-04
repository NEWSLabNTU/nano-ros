//! RTIC action server on STM32F4 — Phase 216.B.5 `nros::main!()` shape.
//!
//! The body collapses to the one-line macro invocation. `nros::main!()`
//! reads `[package.metadata.nros.entry] deploy = "rtic-stm32f4"`
//! from `Cargo.toml`, routes through the RTIC framework branch
//! (Phase 216.B.3), and emits the `#[rtic::app] mod app` body +
//! `__nros_spin` / `__nros_dispatch` RTIC task sidekicks. The board
//! crate (`nros-board-rtic-stm32f4`, Phase 216.B.2) supplies the
//! `RticStm32F4` ZST whose `RticBoardEntry::init_hardware` brings up
//! the hardware and returns the `(Executor, RticRuntime)` pair.
//!
//! ## Deferred dispatch + tag-based action server
//!
//! Unlike the sibling `talker-rtic` (which links `talker_pkg`,
//! `DispatchStrategy::Inline`), this Entry pkg links
//! `stm32f4_action_server_pkg` whose Node declares
//! `DispatchStrategy::Deferred`. The RTIC board's
//! `NodeDispatchRuntime` enqueues signaled goal / cancel / accepted
//! callbacks onto a framework-owned `#[task]`; the action server's
//! `on_callback` body matches against the
//! [`ActionTag`](nros::ActionTag) returned from
//! `create_action_static::<PlaceholderAct>("/fibonacci")`. See
//! `examples/stm32f4/rust/action_server_pkg/src/lib.rs` for the body
//! + the RTIC-side dispatch-handle plumbing TODO.
//!
//! ## Skeleton status
//!
//! `init_hardware`'s body is still `todo!()` (216.B.2 follow-up
//! mirrors the legacy Pattern A bringup), and the trampoline-
//! registration story that hands the sibling
//! `stm32f4_action_server_pkg` Node onto the dispatch runtime —
//! including threading an RTIC `spawn()`-style handle through to
//! `ActionServerState` — is the next 216.B wave. The macro emit +
//! dep graph compile clean today; a real flash will hit the
//! `todo!()` panic in `init_hardware`.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
