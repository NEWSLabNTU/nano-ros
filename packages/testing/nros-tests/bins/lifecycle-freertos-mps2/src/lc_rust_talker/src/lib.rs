//! A managed Rust talker: `std_msgs/Int32` on `/chatter` every 1 s.
//!
//! The launch file marks it `<lifecycle_node>`, and the bringup's
//! `[lifecycle] autostart = "none"` leaves it `unconfigured` at boot, so it
//! must publish nothing until a client configures and activates it. It holds
//! no lifecycle code: the runtime gates its publisher on the node's state.

#![no_std]

use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker — counter state + chatter publish on every tick.
pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let pub_chatter = node.create_publisher_for_topic::<Int32>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    /// Monotonic counter — the next int32 to publish.
    type State = i32;
    // phase-521 W6 — no `tick`, so the entry may park past 10 ms.
    const HAS_TICK: bool = false;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 8>("/chatter", &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);
