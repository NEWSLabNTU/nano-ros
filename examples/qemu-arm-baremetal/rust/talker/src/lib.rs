//! Declarative talker node — RMW/platform-agnostic application logic.
//!
//! Self-contained standalone example (issue 0100): the node logic lives in this
//! crate's `lib.rs` (was a sibling `talker_pkg`), `main.rs` is the
//! `nros::main!()` Form-1 self-bringup Entry that dispatches to this crate's
//! `register`. The boot scaffold (reset → `BoardEntry::run` → executor → spin)
//! is owned by `nros::main!()` + `nros-board-mps2-an385`; none of it appears
//! here.
//!
//! Publishes the `std_msgs/String` demo payload (`Hello World: N`) on `/chatter` once per second,
//! logging `Published: {n}` (the marker the QEMU E2E asserts).

#![no_std]

use core::fmt::Write as _;
use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeResult,
    TickCtx, TimerDuration,
};
use nros_log::{Logger, nros_error, nros_info};
use std_msgs::msg::String as StringMsg;

static LOGGER: Logger = Logger::new("talker");

pub struct TalkerNode;

impl Node for TalkerNode {
    const NAME: &'static str = "talker";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        nros_log::register_logger(&LOGGER);
        let mut node = ctx.create_node(nros::NodeOptions::new("talker"))?;
        node.create_publisher_for_topic::<StringMsg>("/chatter")?;
        node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        Ok(())
    }
}

impl ExecutableNode for TalkerNode {
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut i32, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            // Official ROS 2 demo behavior (phase-277 W4): payload
            // "Hello World: N" (N from 1) + the canonical `Publishing:` line.
            *state = state.wrapping_add(1);
            let mut msg = StringMsg::default();
            let _ = write!(msg.data, "Hello World: {}", *state);
            match ctx.publish_to_topic::<StringMsg, 64>("/chatter", &msg) {
                Ok(()) => nros_info!(&LOGGER, "Publishing: '{}'", msg.data),
                Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
            }
        }
    }

    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

nros::node!(TalkerNode);
