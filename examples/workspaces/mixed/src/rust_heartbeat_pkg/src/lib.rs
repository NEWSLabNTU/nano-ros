//! Rust heartbeat Node pkg for the mixed C/C++/Rust workspace.
//!
//! The package intentionally has no message dependency: it proves that a
//! Rust Node pkg can be linked through the CMake Entry path and registered
//! beside C and C++ Node packages.

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};

pub struct Heartbeat;

impl Node for Heartbeat {
    const NAME: &'static str = "heartbeat";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("heartbeat"))?;
        let timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(500))?;
        node.callback_for_name("on_tick").writes_entity(&timer)?;
        Ok(())
    }
}

impl ExecutableNode for Heartbeat {
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.is_named("on_tick") {
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Heartbeat);
