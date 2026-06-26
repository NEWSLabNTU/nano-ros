//! Declarative serial listener node — RMW/platform/transport-agnostic logic
//! (phase-244.D1). Identical application shape to `listener_pkg`; the only
//! difference between the ethernet and serial listeners is the *transport*,
//! which is a board-build + deploy-overlay concern (the entry pkg builds the
//! board with the `serial` feature + sets a `serial/UART_0#…` locator), never
//! node logic. Declares a `/chatter` subscription bound to `on_message`; each
//! typed `std_msgs/Int32` delivery logs `Received: {data}`.

#![no_std]

use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions,
    NodeResult, TickCtx,
};
use nros_log::{Logger, nros_info};
use std_msgs::msg::Int32;

static LOGGER: Logger = Logger::new("serial_listener");

pub struct SerialListenerNode;

impl Node for SerialListenerNode {
    const NAME: &'static str = "serial_listener";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        nros_log::register_logger(&LOGGER);
        let mut node = ctx.create_node(NodeOptions::new("serial_listener"))?;
        node.create_subscription_for_callback_name::<Int32>("on_message", "/chatter")?;
        nros_info!(&LOGGER, "Subscribing to /chatter (std_msgs/Int32)");
        nros_info!(&LOGGER, "Subscriber declared");
        nros_info!(&LOGGER, "Waiting for messages...");
        Ok(())
    }
}

impl ExecutableNode for SerialListenerNode {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut (), callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_message"
            && let Ok(msg) = ctx.message::<Int32>()
        {
            nros_info!(&LOGGER, "Received: {}", msg.data);
        }
    }

    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

nros::node!(SerialListenerNode);
