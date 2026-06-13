//! FreeRTOS QEMU MPS2-AN385 AddTwoInts service server —
//! Phase 212.L Node pkg.
//!
//! Declarative: node + service server with a `handle_add` callback.
//! Body: reads typed request, writes typed reply through the W.5.3 reply
//! sink. BSP-generated runtime owns init / executor / spin.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};

pub struct AddTwoIntsServer;

impl Node for AddTwoIntsServer {
    const NAME: &'static str = "add_two_ints_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_server"))?;
        let _srv = node.create_service_server_for_name_with_callback::<AddTwoInts>(
            "/add_two_ints",
            "handle_add",
        )?;
        Ok(())
    }
}

impl ExecutableNode for AddTwoIntsServer {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "handle_add" {
            if let Ok(req) = ctx.message::<AddTwoIntsRequest>() {
                let reply = AddTwoIntsResponse { sum: req.a + req.b };
                let _ = ctx.reply::<AddTwoIntsResponse, 64>(&reply);
            }
        }
    }
}

nros::node!(AddTwoIntsServer);
