//! RTIC service client on STM32F4 — Phase 216.B.5 `nros::main!()` shape.
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
//! ## Inline dispatch + future-shaped client
//!
//! Unlike the sibling `service-server-rtic` (which links
//! `stm32f4_service_server_pkg`, `DispatchStrategy::Deferred`), this
//! Entry pkg links `stm32f4_service_client_pkg` whose Node declares
//! `DispatchStrategy::Inline`. The legacy Pattern A client polled
//! `Promise::try_recv()` from a user-owned RTIC `#[task]` — no
//! callbacks fire on the reply path — so a future-shaped client API
//! doesn't need the Deferred runtime trampoline; Inline matches the
//! legacy semantics. See
//! `examples/stm32f4/rust/service_client_pkg/src/lib.rs` for the
//! `register`-only skeleton + the trampoline-registration TODO.
//!
//! ## Skeleton status
//!
//! `init_hardware`'s body is still `todo!()` (216.B.2 follow-up
//! mirrors the legacy Pattern A bringup), and the trampoline-
//! registration story that threads the sibling
//! `stm32f4_service_client_pkg`'s `NodeServiceClient` handle onto
//! `Self::State` (so a tick body can build + send a request and
//! poll the returned `Promise` for the reply) is the next 216.B
//! wave. The macro emit + dep graph compile clean today; a real
//! flash will hit the `todo!()` panic in `init_hardware`.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

nros::main!();
