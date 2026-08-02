//! Phase 228.G fixture — telemetry node (tier `low`).
//!
//! Publishes a monotonic counter on `/telem` every 100 ms so a subscriber can
//! observe the low-tier cadence (Track D) — ~10× slower than the high-tier
//! `/ctrl` node.

#![no_std]

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

pub struct Telem;

impl Node for Telem {
    const NAME: &'static str = "telem_node";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("telem_node"))?;
        node.callback_group("telem")?;
        let pub_telem = node.create_publisher_for_topic::<Int32>("/telem")?;
        let _t =
            node.create_timer_for_callback_name("on_telem", TimerDuration::from_millis(100))?;
        node.callback_for_name("on_telem")
            .publishes_entity(&pub_telem)?;
        Ok(())
    }
}

impl ExecutableNode for Telem {
    /// Monotonic tick counter — the next int32 to publish on `/telem`.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_telem" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 8>("/telem", &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Telem);
