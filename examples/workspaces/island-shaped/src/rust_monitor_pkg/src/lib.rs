//! Rust monitor Node pkg of the island-shaped workspace.
//!
//! Beside the C controller in one image, as the island runs Rust and C
//! components in one system: it subscribes to the controller's command and
//! publishes an alive counter the controller subscribes to.

#![no_std]

use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TimerDuration,
};
use std_msgs::msg::Int32;

pub struct Monitor;

impl Component for Monitor {
    const NAME: &'static str = "monitor";

    // Exact bounds: one publisher.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("monitor"))?;
        let alive = node.create_publisher_for_topic::<Int32>("/monitor/alive")?;
        let _cmd = node.create_subscription_for_callback_name::<Int32>("on_cmd", "/control/cmd")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(100))?;
        node.callback_for_name("on_tick").publishes_entity(&alive)?;
        Ok(())
    }
}

impl ExecutableNode for Monitor {
    /// The last command seen, and the alive counter.
    type State = (i32, i32);
    const HAS_TICK: bool = false;

    fn init() -> Self::State {
        (0, 0)
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_cmd" {
            if let Ok(msg) = ctx.message::<Int32>() {
                state.0 = msg.data;
            }
        } else if callback.as_str() == "on_tick" {
            state.1 = state.1.wrapping_add(1);
            let msg = Int32 { data: state.1 };
            let _ = ctx.publish_to_topic::<Int32, 8>("/monitor/alive", &msg);
        }
    }
}

nros::node!(Monitor);
