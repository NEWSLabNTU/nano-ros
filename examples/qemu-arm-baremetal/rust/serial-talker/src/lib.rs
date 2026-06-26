//! Declarative serial talker node — RMW/platform/transport-agnostic logic
//! (phase-244.D1). Identical application shape to `talker_pkg`; the only
//! difference between the ethernet and serial talkers is the *transport*, which
//! is a board-build + deploy-overlay concern (the entry pkg builds the board
//! with the `serial` feature + sets a `serial/UART_0#…` locator), never node
//! logic. Publishes an incrementing `std_msgs/Int32` on `/chatter` once per
//! second, logging `Published: {n}` (the marker the QEMU E2E asserts).

#![no_std]

use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeResult,
    TickCtx, TimerDuration,
};
use nros_log::{Logger, nros_error, nros_info};
use std_msgs::msg::Int32;

static LOGGER: Logger = Logger::new("serial_talker");

pub struct SerialTalkerNode;

impl Node for SerialTalkerNode {
    const NAME: &'static str = "serial_talker";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        nros_log::register_logger(&LOGGER);
        let mut node = ctx.create_node(nros::NodeOptions::new("serial_talker"))?;
        node.create_publisher_for_topic::<Int32>("/chatter")?;
        node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        Ok(())
    }
}

impl ExecutableNode for SerialTalkerNode {
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut i32, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let count = *state;
            match ctx.publish_to_topic::<Int32, 64>("/chatter", &Int32 { data: count }) {
                Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
            }
            *state = count.wrapping_add(1);
        }
    }

    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

nros::node!(SerialTalkerNode);
