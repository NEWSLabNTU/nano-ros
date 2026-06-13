//! Declarative talker node — RMW/platform-agnostic application logic.
//!
//! Migrated from the legacy single-crate `#[entry] fn main` + `run(Config,
//! closure)` + explicit `Executor` shape (Phase 244.C5). All imperative
//! closure logic — publisher creation, the 1000 ms timer, the publish, and the
//! `nros-log` output — now lives in this declarative node. The boot scaffold
//! (reset → `BoardEntry::run` → executor → spin) is owned by `nros::main!()` +
//! `nros-board-stm32f4`; none of it appears here.
//!
//! Publishes an incrementing `std_msgs/Int32` on `/chatter` once per second,
//! logging `Published: {n}` (the marker the QEMU E2E asserts).

#![no_std]

use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeResult,
    TickCtx, TimerDuration,
};
use nros_log::{Logger, nros_error, nros_info};
use std_msgs::msg::Int32;

static LOGGER: Logger = Logger::new("talker");

pub struct TalkerNode;

impl Node for TalkerNode {
    const NAME: &'static str = "talker";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        nros_log::register_logger(&LOGGER);
        let mut node = ctx.create_node(nros::NodeOptions::new("talker"))?;
        node.create_publisher_for_topic::<Int32>("/chatter")?;
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

nros::node!(TalkerNode);
