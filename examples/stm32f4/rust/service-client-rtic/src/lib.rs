//! STM32F4 Service Client Node pkg.
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
//! This example ships the trait-shaped scaffolding only. The
//! `NodeServiceClient` handle returned by
//! [`NodeContext::create_service_client`](nros::NodeContext::create_service_client)
//! is dropped at the end of `register` — there is no mechanism yet for
//! a Node pkg to hold the handle across calls into a user body (the
//! handle isn't `'static`, and the trampoline-registration story that
//! would thread it onto `Self::State` is a follow-up wave). A
//! real client call body (one fixed request → `Promise::try_recv()`
//! loop → `Result of add_two_ints: <sum>` log) lives as a `// todo`
//! placeholder until then; mirrors the server skeleton's
//! "register-only" stance.
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
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions,
    NodeResult,
};

/// Service client component — calls `/echo` requests. Skeleton.
pub struct ServiceClient;

impl Node for ServiceClient {
    const NAME: &'static str = "add_two_ints_client";

    /// Declares Inline dispatch. The legacy Pattern A client polled
    /// `Promise::try_recv()` from a user-owned RTIC `#[task]` — no
    /// callbacks fire on the reply path, so the dispatch runtime never
    /// needs to deliver one. Inline matches the legacy semantics;
    /// `nros check` accepts the `(RTIC, Inline)` matrix cell.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        // Register the client handle. The metadata-only tag pattern
        // doesn't fit clients (a `ServiceTag` can't issue requests; you
        // need the handle), but the trampoline-registration story for
        // client handles is a follow-up wave — so the returned
        // `NodeServiceClient` is dropped at the end of `register` and
        // the real call body (one fixed request → `Promise::try_recv()`
        // loop → result log) is a `// todo` below.
        let _client = node.create_service_client_for_name::<PlaceholderSrv>("/echo")?;
        // todo(followup): thread `_client` onto `Self::State` once the
        // client-handle trampoline lands, then wire a one-shot call
        // body — build a fixed `PlaceholderInt32` request, call
        // `client.call(&req)`, poll the returned promise, and log
        // `Result of add_two_ints: <sum>` on the reply. The legacy
        // Pattern A loop lived at
        // `examples/stm32f4/rust/service-client-rtic/src/main.rs`
        // (pre-migration) — `let mut promise = client.call(&request)`
        // followed by a `promise.try_recv()` + `Mono::delay` poll.
        Ok(())
    }
}

impl ExecutableNode for ServiceClient {
    /// The next `i32` request value. The real call body is a follow-up
    /// wave; today the field is unused but kept on `State` so the
    /// trampoline-registration follow-up can land without touching the
    /// `init` signature again.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        // Inline dispatch + no Node-registered callbacks (no timer, no
        // subscription) ⇒ this trampoline is never invoked today. Kept
        // as a stub so the `ExecutableNode` trait impl is complete and
        // a follow-up can drop a real one-shot call body in without
        // breaking the `nros::node!()` emit.
        defmt::trace!("add_two_ints_client on_callback (no-op skeleton)");
    }
}

nros::node!(ServiceClient);

// Placeholder service type (issue 0100) — inlined from the former sibling
// `service_server_pkg` so this example is self-contained for copy-out. Minimal
// `RosService` stand-in (Request + Reply both `PlaceholderInt32`, a 4-byte LE
// `i32`) so `create_service_client_for_name` type-checks without dragging in
// `example_interfaces`. The server side keeps an identical copy.
#[derive(Copy, Clone)]
pub struct PlaceholderInt32 {
    pub data: i32,
}

impl nros::Serialize for PlaceholderInt32 {
    fn serialize(&self, _writer: &mut nros::CdrWriter) -> Result<(), nros::SerError> {
        Ok(())
    }
}

impl nros::Deserialize for PlaceholderInt32 {
    fn deserialize(_reader: &mut nros::CdrReader) -> Result<Self, nros::DeserError> {
        Ok(Self { data: 0 })
    }
}

impl nros::RosMessage for PlaceholderInt32 {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int32";
    const TYPE_HASH: &'static str = "";
}

/// Placeholder service — Request + Reply both [`PlaceholderInt32`].
pub struct PlaceholderSrv;

impl nros::RosService for PlaceholderSrv {
    type Request = PlaceholderInt32;
    type Reply = PlaceholderInt32;

    const SERVICE_NAME: &'static str = "service_server_pkg/srv/Echo";
    const SERVICE_HASH: &'static str = "";
}
