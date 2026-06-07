//! Talker Node pkg — publishes `std_msgs/Int32` on `/chatter`.
//!
//! Board-agnostic Node pkg. The sibling Entry pkg (`native_entry`)
//! wires it onto a board via `[package.metadata.nros.entry]` + the
//! `nros::main!(launch = "demo_bringup:...")` macro, which emits one
//! `talker_pkg::register(runtime)?;` call per `<node>` entry in the
//! launch file.
//!
//! `register()` declares the node + a 1 Hz publisher + timer; the
//! `ExecutableNode::on_callback("on_tick")` body bumps a counter and
//! publishes a typed `std_msgs::msg::Int32`. The Entry pkg's
//! macro-generated runtime owns `nros::init`, executor open, RMW
//! registration, and the spin loop.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node_with_options(NodeOptions::new("talker"))?;
        let pub_chatter = node.create_publisher_for_topic::<Int32>("/chatter")?;
        let _timer = node.create_timer_for_callback(
            CallbackId::new("on_tick"),
            TimerDuration::from_millis(1000),
        )?;
        node.callback(CallbackId::new("on_tick"))
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    /// Monotonic counter — the next int32 to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 8>("/chatter", &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);
