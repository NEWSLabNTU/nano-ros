//! Issue 1706 — a talker that declares ONE parameter, so the image builds a
//! parameter store.
//!
//! Where the store comes from is the whole subject. With the contract beside
//! this package declaring `start_value`, the build implies a store and the
//! executor backing carries it: the boot log says
//! `parameter store: N slots (B) carved from the executor backing`. Without
//! the contract the same declaration reaches `leak_parameter_storage` and the
//! store is one heap allocation instead.
//!
//! Every fifth tick logs the Rust global allocator's peak
//! (`nros-platform/alloc-stats`): the heap road's store is a `Box` through
//! that allocator, so the peak is the number the before/after compares.

#![no_std]

use core::fmt::Write as _;
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    ParameterDefault, TimerDuration,
};

use std_msgs::msg::String as StringMsg;

/// Talker with one declared parameter.
pub struct ParamTalker;

impl Node for ParamTalker {
    const NAME: &'static str = "param_talker";

    // One publisher; the parameter services are the executor's, not a cell's.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("param_talker"))?;
        // The declaration that builds the store. Stated in
        // `system.contract.yaml` too, which is what lets the build see it.
        let _start = node
            .declare_parameter_for_name_with_default("start_value", ParameterDefault::Integer(0))?;
        let pub_chatter = node.create_publisher_for_topic::<StringMsg>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for ParamTalker {
    /// Ticks so far.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() != "on_tick" {
            return;
        }
        *state = state.wrapping_add(1);
        // The parameter is READ from the store, so a store that was never
        // built shows up as `-1` in the payload rather than passing quietly.
        let start = ctx.parameter::<i64>("start_value").unwrap_or(-1);
        let mut msg = StringMsg::default();
        let _ = write!(msg.data, "Hello World: {}", i64::from(*state) + start);
        let _ = ctx.publish_to_topic::<StringMsg, 64>("/chatter", &msg);
        log::info!("Publishing: '{}'", msg.data);
        #[cfg(any(target_os = "nuttx", target_os = "none"))]
        if *state % 5 == 1 {
            log::info!(
                "rust heap peak: {} bytes (in use {})",
                nros_platform::heap_stats::peak(),
                nros_platform::heap_stats::used()
            );
        }
    }
}

nros::node!(ParamTalker);
