//! ThreadX Linux Listener — Phase 212.L Node pkg.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and tracks the last seen
//! value. The generated runtime owns init / executor / spin.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
};
use std_msgs::msg::Int32;

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub = node.create_subscription::<Int32>(
            EntityId::new("sub_chatter"),
            CallbackId::new("on_chatter"),
            "/chatter",
        )?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
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

nros::node!(Listener);
