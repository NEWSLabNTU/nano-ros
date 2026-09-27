//! Talker Node pkg — publishes `std_msgs/Int32` on `/chatter` (phase-263 B3).
//!
//! It is the bridge's source, and it is a plain talker: which session it
//! publishes on, and where `/chatter` is forwarded to, are declared by the
//! bringup's `[[domain]]` / `[[bridge]]` rows and emitted by
//! `nros codegen system`. See `demo_bringup/system.toml`.

#![no_std]

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

pub struct Talker;

impl Node for Talker {
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
    type State = i32;

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
