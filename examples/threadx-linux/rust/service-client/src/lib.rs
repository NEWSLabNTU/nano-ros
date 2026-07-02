//! ThreadX Linux Service Client — Node pkg.
//!
//! Declares a service client for `example_interfaces/AddTwoInts` on
//! `/add_two_ints` and sends ONE fixed request (2, 3) from `tick`.
//! A failed call (server not yet discovered) retries on the next tick;
//! once the reply lands the client logs the sum and goes quiet. The
//! generated runtime owns init / executor / spin.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
};

pub struct ServiceClient;

impl Node for ServiceClient {
    const NAME: &'static str = "service_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        let _client = node.create_service_client_for_name::<AddTwoInts>("/add_two_ints")?;
        Ok(())
    }
}

pub struct State {
    /// Set once the reply has been received — the client sends ONE request.
    done: bool,
}

impl ExecutableNode for ServiceClient {
    type State = State;

    fn init() -> Self::State {
        State { done: false }
    }

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.done {
            return;
        }
        let req = AddTwoIntsRequest { a: 2, b: 3 };
        if let Ok(resp) = ctx
            .call_for_name::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>("/add_two_ints", &req)
        {
            log::info!("Result of add_two_ints: {}", resp.sum);
            state.done = true;
        }
        // On failure (server not yet discovered) the next tick retries.
    }
}

nros::node!(ServiceClient);
