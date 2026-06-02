//! NuttX QEMU ARM Listener — Phase 212.L Component pkg.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and tracks the last seen
//! value. The generated runtime owns init / executor / spin.

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
    /// Last value seen on `/chatter` (state shared across callback ticks).
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

/// Phase 212.N.7 step-2 — codegen-facing `register` entry point.
///
/// See the `talker` Component pkg sibling for full docs. Generic over
/// `R: ?Sized` so the Component pkg's Cargo.toml does not need a
/// direct `nros-platform` dep; the Entry pkg monomorphises `R` to
/// `nros_platform::RuntimeCtx<'_>`. Body is a no-op until the 212.N
/// runtime plumbing lands.
pub fn register<R: ?Sized>(_runtime: &mut R) -> Result<(), &'static str> {
    Ok(())
}
