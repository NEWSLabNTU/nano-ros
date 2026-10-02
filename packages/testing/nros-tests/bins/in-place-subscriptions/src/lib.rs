//! Issue 1340 fixture — eight subscriptions an in-place backend dispatches
//! without a receive region.
//!
//! Each registers through `create_subscription_for_callback_name`, the
//! generic path that asks the backend for in-place dispatch, on its own
//! `/chatterN` topic, and the callback prints `I heard: [...]` with the topic
//! number so a run can show every subscription delivering on the smaller arena.

#![no_std]

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::String as StringMsg;

pub struct EightListener;

const TOPICS: [(&str, &str); 8] = [
    ("on_chatter1", "/chatter1"),
    ("on_chatter2", "/chatter2"),
    ("on_chatter3", "/chatter3"),
    ("on_chatter4", "/chatter4"),
    ("on_chatter5", "/chatter5"),
    ("on_chatter6", "/chatter6"),
    ("on_chatter7", "/chatter7"),
    ("on_chatter8", "/chatter8"),
];

impl Node for EightListener {
    const NAME: &'static str = "eight_listener";

    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("eight_listener"))?;
        for (callback, topic) in TOPICS {
            let _sub = node.create_subscription_for_callback_name::<StringMsg>(callback, topic)?;
            log::info!("Subscriber created for topic: {}", topic);
        }
        Ok(())
    }
}

impl ExecutableNode for EightListener {
    /// Messages seen, across all eight topics.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        let Some(n) = TOPICS.iter().position(|(cb, _)| *cb == callback.as_str()) else {
            return;
        };
        if let Ok(msg) = ctx.message::<StringMsg>() {
            *state = state.wrapping_add(1);
            log::info!("I heard on /chatter{}: [{}]", n + 1, msg.data);
        }
    }
}

nros::node!(EightListener);
