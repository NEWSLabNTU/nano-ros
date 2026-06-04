//! RTIC action client on STM32F4 — Phase 216.B.5 `nros::main!()` shape.
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
//! ## Inline dispatch — client-side rationale
//!
//! Unlike the sibling `action-server-rtic` (Deferred), this Entry pkg
//! links `stm32f4_action_client_pkg` whose Node declares
//! `DispatchStrategy::Inline`. The client side has no callbacks on
//! the spin path — the legacy Pattern A `main.rs` (pre-migration)
//! polled `try_recv()` / `try_recv_feedback()` from an `async fn`
//! task to drive goal-accept / feedback / result. Inline matches
//! the `talker-rtic` matrix cell `nros check` (Phase 216.D.1)
//! already accepts. The integration story (where the send_goal +
//! poll bodies live now that the legacy `#[task]` is gone) is the
//! follow-up B wave documented in the sibling Node pkg's
//! `src/lib.rs`.
//!
//! ## PlaceholderAct reuse
//!
//! `stm32f4_action_client_pkg` reuses `PlaceholderAct` from
//! `stm32f4_action_server_pkg` (transitive dep) so client + server
//! wire shapes stay aligned by construction. When the real
//! `example_interfaces::action::Fibonacci` lands, both pkgs flip
//! together.
//!
//! ## Skeleton status
//!
//! `init_hardware`'s body is still `todo!()` (216.B.2 follow-up
//! mirrors the legacy Pattern A bringup), and the trampoline-
//! registration story that hands the sibling
//! `stm32f4_action_client_pkg` Node onto the dispatch runtime —
//! including the framework-owned `#[task]` that drives the
//! `try_recv*` loops in place of the legacy hand-rolled task — is
//! the next 216.B wave. The macro emit + dep graph compile clean
//! today; a real flash will hit the `todo!()` panic in
//! `init_hardware`.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
