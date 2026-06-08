//! ThreadX Linux Service Client — Phase 212.L Node pkg.
//!
//! Declares a service client for `example_interfaces/AddTwoInts` on
//! `/add_two_ints`. Phase 212.M-F.4.b transcription: one-shot
//! `send_request` on the first `tick` call. Until the M-F.4.a
//! `GenClientDispatch` reaches the installed nros-cli, the in-tree
//! `UnsupportedClients` stub returns `NodeDeclError::Runtime`; the
//! body still compiles + the seam is honest.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TickCtx,
};

pub struct ServiceClient;

impl Node for ServiceClient {
    const NAME: &'static str = "service_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        let _client =
            node.create_service_client::<AddTwoInts>(EntityId::new("cli_add"), "/add_two_ints")?;
        Ok(())
    }
}

pub struct State {
    /// Set once the call has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableNode for ServiceClient {
    type State = State;

    fn init() -> Self::State {
        State { sent: false }
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let req = AddTwoIntsRequest { a: 7, b: 35 };
        let result: nros::NodeResult<AddTwoIntsResponse> = ctx
            .call::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>(EntityId::new("cli_add"), &req);
        if result.is_ok() {
            state.sent = true;
        }
    }
}

nros::node!(ServiceClient);
