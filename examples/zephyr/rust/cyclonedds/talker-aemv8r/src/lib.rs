//! Phase 217.D.1 — Rust talker on the ARM FVP `Base_RevC AEMv8-R`
//! Cortex-A SMP target (Zephyr 3.7).
//!
//! Rust-side sibling of `examples/zephyr/cpp/cyclonedds/talker-aemv8r/`:
//! same `std_msgs/Int32` payload on `/chatter` so the FVP runtime
//! verification (Phase 217.A run recipes + Phase 217.C smoke) covers
//! both languages from a single peer.
//!
//! Node-pkg shape mirrors `examples/zephyr/rust/talker/src/lib.rs`
//! (Phase 212.M.3 / 212.L Component pkg). `register` declares the node
//! + publisher + 1 Hz timer; `ExecutableNode::on_callback` runs the
//! timer body (bump counter, publish). `nros::zephyr_component_main!(Talker)`
//! owns executor open, node registration, and the spin loop for this
//! self-package Rust application.
//!
//! Board glue (BOARD / per-board prj.conf / DTS overlay / default RMW)
//! comes from `nano_ros_use_board(fvp-aemv8r-smp)` in `CMakeLists.txt`
//! (Phase 215.B contract).

#![no_std]

extern crate zephyr;

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("aemv8r_talker"))?;
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

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 64>("/chatter", &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);
nros::zephyr_component_main!(Talker);
