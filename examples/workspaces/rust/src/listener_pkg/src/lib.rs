//! Listener Node pkg — subscribes to `std_msgs/Int32` on `/chatter`.
//!
//! Board-agnostic Node pkg. The sibling Entry pkg (`native_entry`)
//! wires it onto a board via the `nros::main!(launch = "demo_bringup:...")`
//! macro, which emits one `listener_pkg::register(runtime)?;` call per
//! matching `<node>` entry in the launch file.
//!
//! `register()` declares the node + a subscription whose `on_message`
//! callback decodes the incoming `std_msgs::msg::Int32` payload.

#![no_std]

use nros::{CallbackCtx, CallbackId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::Int32;

/// Listener — counts the int32 messages seen on `/chatter`.
pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node_with_options(NodeOptions::new("listener"))?;
        let sub_chatter = node
            .create_subscription_for_callback::<Int32>(CallbackId::new("on_message"), "/chatter")?;
        node.callback(CallbackId::new("on_message"))
            .reads_entity(&sub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Count of messages received so far.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_message" {
            if ctx.message::<Int32>().is_ok() {
                *state = state.wrapping_add(1);
            }
        }
    }
}

nros::node!(Listener);
