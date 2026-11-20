//! Talker Node pkg — publishes `std_msgs/String` (`Hello World: N`) on
//! `/chatter` once per second, and logs the `Publishing:` line the e2e
//! harness counts. It names no RMW: the bringup's image does.

#![no_std]

use core::fmt::Write as _;
use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TimerDuration,
};
use std_msgs::msg::String as StringMsg;

pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";

    // issue 0857 — exact bounds: one publisher.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let pub_chatter = node.create_publisher_for_topic::<StringMsg>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    type State = i32;
    const HAS_TICK: bool = false;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            *state = state.wrapping_add(1);
            let mut msg = StringMsg::default();
            let _ = write!(msg.data, "Hello World: {}", *state);
            let _ = ctx.publish_to_topic::<StringMsg, 64>("/chatter", &msg);
            log::info!("Publishing: '{}'", msg.data);
        }
    }
}

nros::node!(Talker);
