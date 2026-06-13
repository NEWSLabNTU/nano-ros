//! ESP32-C3 QEMU Listener — Node pkg (agnostic application logic).
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and tracks the last
//! value seen.
//!
//! Node pkg shape: `register()` declares the node + subscription;
//! `ExecutableNode::on_callback("on_chatter")` runs the subscription
//! body. The board crate's `BoardEntry` runtime owns `Executor::open`,
//! RMW registration, hardware/transport bring-up and the spin loop —
//! this source carries only the listener behaviour.

#![no_std]

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::Int32;

/// Listener component — last value seen on `/chatter`.
pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub = node.create_subscription_for_callback_name::<Int32>("on_chatter", "/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Last value seen on `/chatter` (state shared across callback ticks).
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if let Ok(msg) = ctx.message::<Int32>() {
                *state = msg.data;
            }
        }
    }
}

nros::node!(Listener);
