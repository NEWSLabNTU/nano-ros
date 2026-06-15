//! ESP32-C3 QEMU Talker — Node pkg (agnostic application logic).
//!
//! Publishes `std_msgs/Int32` on `/chatter` once per second.
//!
//! Node pkg shape: `register()` declares the node + publisher + timer;
//! `ExecutableNode::on_callback("on_tick")` runs the timer body (bump
//! counter, publish). The board crate's `BoardEntry` runtime owns
//! `Executor::open`, RMW registration, hardware/transport bring-up and
//! the spin loop — this source carries only the talker behaviour.

#![no_std]

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker component — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let pub_chatter = node.create_publisher_for_topic::<Int32>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
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

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 64>("/chatter", &msg);
            // Observable per-publish line (routed to the console by the board's
            // log writer) — the e2e harness waits for `Published:` to confirm the
            // 1 Hz timer fired + the session published. Mirrors native examples.
            log::info!("Published: {}", *state);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);
