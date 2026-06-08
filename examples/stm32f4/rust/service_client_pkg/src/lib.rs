//! STM32F4 Service Client Node pkg — Phase 216.B.5.
//!
//! Board-agnostic Node pkg that the sibling Entry pkg
//! (`service-client-rtic` for the RTIC framework — and the same pkg
//! is reusable from a future `service-client-embassy` Entry pkg if
//! the Embassy wave grows that example) wires onto a board +
//! framework via its `[package.metadata.nros.entry] deploy = "<board>"`
//! key plus the `nros::main!()` macro.
//!
//! ## Inline dispatch + future-shaped client
//!
//! Unlike the sibling `service_server_pkg` (`DispatchStrategy::Deferred`
//! — request arrival is a callback the dispatch runtime hands off to a
//! framework-owned task), this pkg declares
//!
//! ```ignore
//! const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;
//! ```
//!
//! The legacy Pattern A `service-client-rtic` (`src/main.rs` before
//! this migration) polled `Promise::try_recv()` from a user-owned RTIC
//! `#[task]` — no callbacks fire on the reply path. A future-shaped
//! client API doesn't need the Deferred runtime trampoline, so Inline
//! matches the legacy semantics and `nros check` accepts the
//! `(RTIC, Inline)` matrix cell.
//!
//! ## Skeleton status — `register()` only
//!
//! Phase 216.B.5 ships the trait-shaped scaffolding only. The
//! `NodeServiceClient` handle returned by
//! [`NodeContext::create_service_client`](nros::NodeContext::create_service_client)
//! is dropped at the end of `register` — there is no mechanism yet for
//! a Node pkg to hold the handle across calls into a user body (the
//! handle isn't `'static`, and the trampoline-registration story that
//! would thread it onto `Self::State` is the next 216.B wave). A
//! real client call body (request → `Promise::try_recv()` loop →
//! reply log) lives as a `// todo` placeholder until then; mirrors
//! the server skeleton's "register-only" stance.
//!
//! ## Placeholder service type
//!
//! Re-uses the sibling [`stm32f4_service_server_pkg::PlaceholderSrv`]
//! `RosService` impl directly so the wire shape (Request + Reply both
//! shaped like `std_msgs/Int32` — 4-byte LE `i32`) is guaranteed to
//! match across both sides without dragging `example_interfaces` codegen
//! into either skeleton. A follow-up swaps both pkgs to a typed
//! `AddTwoInts` (or similar `example_interfaces::srv::*`) once
//! `generated/` ships.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, DispatchStrategy, EntityId, ExecutableNode, Node, NodeContext,
    NodeOptions, NodeResult,
};
use stm32f4_service_server_pkg::PlaceholderSrv;

/// Service client component — calls `/echo` requests. Phase 216.B.5
/// skeleton.
pub struct ServiceClient;

impl Node for ServiceClient {
    const NAME: &'static str = "service_client";

    /// Phase 216.B.5 — declares Inline dispatch. The legacy Pattern A
    /// client polled `Promise::try_recv()` from a user-owned RTIC
    /// `#[task]` — no callbacks fire on the reply path, so the
    /// dispatch runtime never needs to deliver one. Inline matches
    /// the legacy semantics; `nros check` (Phase 216.D.1) accepts the
    /// `(RTIC, Inline)` matrix cell.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("service_client"))?;
        // Phase 216.B.5 — register the client handle. The metadata-only
        // tag pattern doesn't fit clients (a `ServiceTag` can't issue
        // requests; you need the handle), but Phase 216.B's
        // trampoline-registration story for client handles is the next
        // wave — so the returned `NodeServiceClient` is dropped at the
        // end of `register` and the real call body (request →
        // `Promise::try_recv()` loop → reply log) is a `// todo`
        // below.
        let _client =
            node.create_service_client::<PlaceholderSrv>(EntityId::new("cli_echo"), "/echo")?;
        // todo(216.B.5-followup): thread `_client` onto `Self::State`
        // once the client-handle trampoline lands, then wire a
        // periodic call body — e.g. a tick callback that builds a
        // `PlaceholderInt32 { data: counter }` request, calls
        // `client.call(&req)`, polls the returned promise to logged
        // reply. The legacy Pattern A loop lived at
        // `examples/stm32f4/rust/service-client-rtic/src/main.rs`
        // (pre-migration) — `let mut promise = client.call(&request)`
        // followed by a `promise.try_recv()` + `Mono::delay` poll.
        Ok(())
    }
}

impl ExecutableNode for ServiceClient {
    /// Monotonic counter — the next `i32` request value. The real call
    /// body is the next 216.B wave; today the field is unused but
    /// kept on `State` so the trampoline-registration follow-up can
    /// land without touching the `init` signature again.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // Phase 216.B.5 — Inline dispatch + no Node-registered
        // callbacks (no timer, no subscription) ⇒ this trampoline is
        // never invoked today. Kept as a stub so the `ExecutableNode`
        // trait impl is complete and a follow-up can drop a real
        // tick-driven call body in without breaking the
        // `nros::node!()` emit.
        defmt::trace!("service_client on_callback (no-op skeleton)");
    }
}

nros::node!(ServiceClient);
