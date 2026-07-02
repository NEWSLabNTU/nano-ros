//! ThreadX Linux Service Server — Node pkg.
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.
//! The body deserializes the request from `CallbackCtx::message`, sums
//! the two ints, and writes the typed reply via `CallbackCtx::reply`.
//! The generated runtime owns init / executor / spin.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};

pub struct ServiceServer;

impl Node for ServiceServer {
    const NAME: &'static str = "service_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_server"))?;
        let _srv = node.create_service_server_for_name_with_callback::<AddTwoInts>(
            "/add_two_ints",
            "on_add",
        )?;
        // Readiness marker the e2e harness greps before driving the client.
        log::info!("Waiting for service requests");
        Ok(())
    }
}

impl ExecutableNode for ServiceServer {
    /// Count of handled requests.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_add"
            && let Ok(req) = ctx.message::<AddTwoIntsRequest>()
        {
            log::info!("Incoming request");
            log::info!("a: {} b: {}", req.a, req.b);
            let resp = AddTwoIntsResponse { sum: req.a + req.b };
            let _ = ctx.reply::<AddTwoIntsResponse, 64>(&resp);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(ServiceServer);
