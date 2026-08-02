//! Phase 228.G fixture — control node (tier `high`).
//!
//! Labels its timer with the `ctrl` callback group; `system.toml` maps that
//! group to the `high` tier. `nros::node!` emits the `register` symbol the
//! `nros::main!()` per-tier emit calls. Publishes a monotonic counter on `/ctrl`
//! every 10 ms so a subscriber can observe the high-tier cadence (Track D).

#![no_std]

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

pub struct Control;

impl Node for Control {
    const NAME: &'static str = "control_node";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("control_node"))?;
        node.callback_group("ctrl")?;
        let pub_ctrl = node.create_publisher_for_topic::<Int32>("/ctrl")?;
        let _t = node.create_timer_for_callback_name("on_ctrl", TimerDuration::from_millis(10))?;
        node.callback_for_name("on_ctrl")
            .publishes_entity(&pub_ctrl)?;
        Ok(())
    }
}

impl ExecutableNode for Control {
    /// Monotonic tick counter — the next int32 to publish on `/ctrl`.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_ctrl" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 8>("/ctrl", &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Control);
