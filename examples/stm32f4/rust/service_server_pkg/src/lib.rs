//! STM32F4 Service Server Node pkg — Phase 216.B.5.
//!
//! Board-agnostic Node pkg that the sibling Entry pkg
//! (`service-server-rtic` for the RTIC framework — and the same pkg
//! is reusable from a future `service-server-embassy` Entry pkg if
//! the Embassy wave grows that example) wires onto a board +
//! framework via its `[package.metadata.nros.entry] deploy = "<board>"`
//! key plus the `nros::main!()` macro.
//!
//! ## Deferred dispatch + tag-based service server
//!
//! Like the sibling `listener_pkg` (216.C.5), this pkg declares
//!
//! ```ignore
//! const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
//! ```
//!
//! Service callbacks are exactly the Deferred-dispatch use case: a
//! request arrives, the board-side dispatch runtime (RTIC: an SPSC
//! ring drained by a `#[task]`; Embassy: a framework-owned queue)
//! enqueues the signaled callback onto a framework-owned task
//! instead of the spin task, and the handler body runs there. The
//! server is registered via the tag-shaped
//! [`NodeContext::create_service_static`](nros::NodeContext::create_service_static)
//! helper landed in the 216.A.4-followup, which returns a
//! [`ServiceTag`] the Node author stores on `Self::State` and matches
//! against the `Callback<'_>` delivered to
//! [`ExecutableNode::on_callback`].
//!
//! ## Placeholder service type
//!
//! Phase 216.B.5 ships the trait-shaped scaffolding only. The body
//! uses a tiny local [`PlaceholderSrv`] `RosService` impl (Request +
//! Reply both shaped like `std_msgs/Int32` — 4-byte LE `i32`),
//! avoiding a dep on `example_interfaces` (which would require
//! `nros ws sync` to materialise `generated/example_interfaces/`
//! before cross-check). A follow-up that finishes the trampoline-
//! registration story swaps this for a typed `AddTwoInts` (or
//! similar `example_interfaces::srv::*`) once `generated/` ships.
//!
//! The original Pattern A `service-server-rtic` (before this
//! migration) drove `example_interfaces::srv::AddTwoInts` through an
//! `EmbeddedServiceServer<S>::handle_request(|req| Response { … })`
//! polling loop on an RTIC `#[task]`. Under the macro shape, the
//! handler body lives in `on_callback` and runs on the dispatch
//! runtime's task — the framework owns the polling loop.

#![no_std]

use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions,
    NodeResult, ServiceTag,
};

/// Service server component — answers `/echo` requests. Phase 216.B.5
/// skeleton.
pub struct ServiceServer;

/// Per-instance mutable state. Holds the [`ServiceTag`] returned from
/// registration (used in `on_callback` to match incoming request
/// callbacks).
pub struct ServiceServerState {
    /// Tag returned from `create_service_static::<PlaceholderSrv>("/echo")`.
    /// Macro-emitted init bodies use [`ServiceTag::placeholder`] as a
    /// sentinel; the real tag is bound at register time by a follow-
    /// up wave of Phase 216.B.
    pub srv_echo: ServiceTag,
}

impl Node for ServiceServer {
    const NAME: &'static str = "service_server";

    /// Phase 216.B.5 — declares Deferred dispatch. Service callbacks
    /// are the canonical Deferred-dispatch use case: a request hits
    /// the wire, the dispatch runtime enqueues the callback onto a
    /// framework-owned task, the handler body builds the reply
    /// off the spin task. `nros check` (Phase 216.D.1) accepts the
    /// `(RTIC, Deferred)` matrix cell.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("service_server"))?;
        // Phase 216.B.5 — tag-based service server. The service-name
        // literal becomes both the stable entity ID and the callback
        // ID; the returned `ServiceTag` is what `on_callback` matches
        // against the delivered `Callback<'_>`. See the module doc
        // for the Deferred dispatch rationale.
        let _srv_echo = node.create_service_static::<PlaceholderSrv>("/echo")?;
        Ok(())
    }
}

impl ExecutableNode for ServiceServer {
    type State = ServiceServerState;

    fn init() -> Self::State {
        // Phase 216.B.5 — `srv_echo` uses `ServiceTag::placeholder()`
        // as the macro-emit sentinel; the real tag (returned by
        // `create_service_static` in `register`) is bound at register
        // time by the follow-up wave that finishes the trampoline-
        // registration story.
        ServiceServerState {
            srv_echo: ServiceTag::placeholder(),
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if state.srv_echo == callback {
            // 4-byte LE decode == wire shape of `PlaceholderSrv`'s
            // Request (mirrors `std_msgs/Int32`; CDR-PL, header
            // omitted — placeholder; the RTIC dispatch path is not
            // yet hooked up, so this never reaches the wire on a
            // real flash). A real handler would build a reply via
            // `ctx.send_reply(...)` (the typed surface lands with
            // the trampoline-registration follow-up); for the
            // skeleton the request is decoded and logged only.
            let payload = ctx.payload();
            let req = decode_placeholder_int32(payload);
            defmt::info!("Service request received: {}", req.data);
        }
    }
}

/// Decode a [`PlaceholderInt32`] from a raw CDR payload. 4-byte LE
/// `i32` (header omitted in the placeholder; mirrors `listener_pkg`'s
/// decode path).
fn decode_placeholder_int32(payload: &[u8]) -> PlaceholderInt32 {
    let mut bytes = [0u8; 4];
    let n = core::cmp::min(payload.len(), 4);
    bytes[..n].copy_from_slice(&payload[..n]);
    PlaceholderInt32 {
        data: i32::from_le_bytes(bytes),
    }
}

nros::node!(ServiceServer);

// Phase 216.B.5 placeholder — minimal `RosMessage` stand-in (shared
// between Request + Reply) so the declarative `create_service_static`
// call type-checks without dragging `example_interfaces` (which is
// codegen-materialised under `generated/example_interfaces/`) into
// this skeleton. The wire shape matches `std_msgs/Int32`. Follow-ups
// switch to the real types once `generated/` ships for this example.
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

// Phase 216.B.5 placeholder — minimal `RosService` stand-in so the
// declarative `create_service_static::<PlaceholderSrv>(...)` call
// type-checks without dragging `example_interfaces` into this
// skeleton. Request + Reply are both `PlaceholderInt32` (a 4-byte LE
// `i32`); a real flash would swap this for `example_interfaces::srv::
// AddTwoInts` once the trampoline-registration story lands and
// `generated/` ships.
pub struct PlaceholderSrv;

impl nros::RosService for PlaceholderSrv {
    type Request = PlaceholderInt32;
    type Reply = PlaceholderInt32;

    const SERVICE_NAME: &'static str = "service_server_pkg/srv/Echo";
    const SERVICE_HASH: &'static str = "";
}
