//! ThreadX Linux Service Server — Phase 212.L Component pkg.
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.
//! The body deserializes the request from `CallbackCtx::message`, sums
//! the two ints, and writes the typed reply via `CallbackCtx::reply`.
//! The generated runtime owns init / executor / spin.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions,
};

pub struct ServiceServer;

impl Component for ServiceServer {
    const NAME: &'static str = "service_server";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node =
            ctx.create_node(NodeId::new("node"), NodeOptions::new("add_two_ints_server"))?;
        let _srv = node.create_service_server::<AddTwoInts>(
            EntityId::new("srv_add"),
            CallbackId::new("on_add"),
            "/add_two_ints",
        )?;
        Ok(())
    }
}

impl ExecutableComponent for ServiceServer {
    /// Count of handled requests.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_add" {
            if let Ok(req) = ctx.message::<AddTwoIntsRequest>() {
                let resp = AddTwoIntsResponse { sum: req.a + req.b };
                let _ = ctx.reply::<AddTwoIntsResponse, 64>(&resp);
                *state = state.wrapping_add(1);
            }
        }
    }
}

nros::component!(ServiceServer);
