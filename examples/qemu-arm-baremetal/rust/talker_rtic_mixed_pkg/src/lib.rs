//! Declarative mixed-priority RTIC talker node — RMW/platform-agnostic logic.
//!
//! Migrated from the legacy hand-written `#[rtic::app]` + `#[init]`/`#[task]` +
//! manual `Executor` shape (Phase 244.D1). The old example created the
//! publisher in `#[init]`, drove transport I/O from a low-priority `net_poll`
//! task, and published from a *higher*-priority `publish` task on a 1000 ms
//! `Mono::delay` — the `ffi-sync` feature masking interrupts around FFI calls so
//! the preempting publisher could not corrupt zenoh-pico's global state.
//!
//! In the declarative shape there is a single dispatch context, so the explicit
//! two-priority split (low `net_poll` vs high `publish`) does not survive the
//! migration — the boot scaffold owns spinning and the timer fires the publish
//! callback cooperatively. The pub/sub behavior and output markers are
//! preserved: an incrementing `std_msgs/Int32` on `/chatter` once per second,
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
        nros_info!(&LOGGER, "Publishing messages...");
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
