// Issue 2812 -- the one parameter-serving talker the five packages of this
// fixture each export, `include!`d by every package's `lib.rs`.
//
// One package exports ONE node (`nros::node!` emits the package's
// `register`), so five nodes are five packages; this file is what they share.

use core::fmt::Write as _;
use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, ParameterDefault, TimerDuration,
};

use std_msgs::msg::String as StringMsg;

/// One parameter-serving talker: node `$node`, parameter `start_value`,
/// publishing on `$topic`.
macro_rules! param_node {
    ($ty:ident, $node:literal, $topic:literal, $report:literal) => {
        #[doc = concat!("The `", $node, "` node: one parameter, one publisher on `", $topic, "`.")]
        pub struct $ty;

        impl Component for $ty {
            const NAME: &'static str = $node;

            // One publisher; the parameter services are the executor's.
            const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

            fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
                let mut node = ctx.create_node(NodeOptions::new($node))?;
                let _start = node.declare_parameter_for_name_with_default(
                    "start_value",
                    ParameterDefault::Integer(0),
                )?;
                let publisher = node.create_publisher_for_topic::<StringMsg>($topic)?;
                let _timer = node
                    .create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
                node.callback_for_name("on_tick")
                    .publishes_entity(&publisher)?;
                Ok(())
            }
        }

        impl ExecutableNode for $ty {
            /// Ticks so far.
            type State = i32;
            const HAS_TICK: bool = false;

            fn init() -> Self::State {
                0
            }

            fn on_callback(
                state: &mut Self::State,
                callback: Callback<'_>,
                ctx: &mut CallbackCtx<'_>,
            ) {
                if callback.as_str() != "on_tick" {
                    return;
                }
                *state = state.wrapping_add(1);
                let start = ctx.parameter::<i64>("start_value").unwrap_or(-1);
                let mut msg = StringMsg::default();
                let _ = write!(msg.data, "Hello World: {}", i64::from(*state) + start);
                let _ = ctx.publish_to_topic::<StringMsg, 64>($topic, &msg);
                if $report {
                    log::info!("Publishing: '{}'", msg.data);
                }
            }
        }
    };
}
