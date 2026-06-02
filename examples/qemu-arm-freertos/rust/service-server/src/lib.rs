//! FreeRTOS QEMU MPS2-AN385 AddTwoInts service server —
//! Phase 212.L Component pkg.
//!
//! Declarative: node + service server with a `handle_add` callback.
//! Body: reads typed request, writes typed reply through the W.5.3 reply
//! sink. BSP-generated runtime owns init / executor / spin.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions,
};

pub struct AddTwoIntsServer;

impl Component for AddTwoIntsServer {
    const NAME: &'static str = "add_two_ints_server";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node =
            ctx.create_node(NodeId::new("node"), NodeOptions::new("add_two_ints_server"))?;
        let _srv = node.create_service_server::<AddTwoInts>(
            EntityId::new("srv_add"),
            CallbackId::new("handle_add"),
            "/add_two_ints",
        )?;
        Ok(())
    }
}

impl ExecutableComponent for AddTwoIntsServer {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "handle_add" {
            if let Ok(req) = ctx.message::<AddTwoIntsRequest>() {
                let reply = AddTwoIntsResponse { sum: req.a + req.b };
                let _ = ctx.reply::<AddTwoIntsResponse, 64>(&reply);
            }
        }
    }
}

nros::component!(AddTwoIntsServer);

/// Phase 212.N.7 step-2 — Entry-pkg-facing register wrapper.
///
/// TODO stub: see `freertos_rs_talker::register` for the rationale.
/// `RuntimeCtx` does not yet expose a `ComponentRuntime` sink, so the
/// existing `<AddTwoIntsServer as Component>::register(ctx)` machinery
/// wired by `nros::component!(AddTwoIntsServer)` cannot be driven from
/// here. The live registration path remains the macro-emitted
/// `nros_component_register` extern that the FreeRTOS BSP baker
/// discovers at link time.
///
/// Generic over `R` to avoid adding an `nros-platform` direct dep —
/// step-2 contract kept `Cargo.toml` untouched. Entry pkg passes
/// `&mut nros_platform::RuntimeCtx<'_>`.
pub fn register<R>(_runtime: &mut R) -> Result<(), &'static str> {
    // TODO(212.N.7 step-3+): wire to <AddTwoIntsServer as Component>::register
    // once RuntimeCtx exposes a ComponentRuntime sink.
    Ok(())
}
