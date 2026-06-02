//! FreeRTOS QEMU MPS2-AN385 Listener — Phase 212.L Component pkg.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and tracks the last seen
//! value. The BSP-generated runtime owns init / executor / spin.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions,
};
use std_msgs::msg::Int32;

pub struct Listener;

impl Component for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(NodeId::new("node"), NodeOptions::new("listener"))?;
        let _sub = node.create_subscription::<Int32>(
            EntityId::new("sub_chatter"),
            CallbackId::new("on_chatter"),
            "/chatter",
        )?;
        Ok(())
    }
}

impl ExecutableComponent for Listener {
    /// Last value seen on `/chatter`.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if let Ok(msg) = ctx.message::<Int32>() {
                *state = msg.data;
            }
        }
    }
}

nros::component!(Listener);

/// Phase 212.N.7 step-2 — Entry-pkg-facing register wrapper.
///
/// TODO stub: see `freertos_rs_talker::register` for the rationale.
/// `RuntimeCtx` does not yet expose a `ComponentRuntime` sink, so the
/// existing `<Listener as Component>::register(ctx)` machinery wired
/// by `nros::component!(Listener)` cannot be driven from here. The
/// live registration path remains the macro-emitted
/// `nros_component_register` extern that the FreeRTOS BSP baker
/// discovers at link time.
///
/// Generic over `R` to avoid adding an `nros-platform` direct dep —
/// step-2 contract kept `Cargo.toml` untouched. Entry pkg passes
/// `&mut nros_platform::RuntimeCtx<'_>`.
pub fn register<R>(_runtime: &mut R) -> Result<(), &'static str> {
    // TODO(212.N.7 step-3+): wire to <Listener as Component>::register
    // once RuntimeCtx exposes a ComponentRuntime sink.
    Ok(())
}
