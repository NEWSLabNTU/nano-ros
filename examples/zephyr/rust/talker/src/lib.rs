//! Zephyr Talker — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Publishes `std_msgs/Int32` on `/chatter` once per second.
//!
//! Node pkg shape: `register()` declares node + publisher + timer;
//! `ExecutableNode::on_callback("on_tick")` runs the timer body
//! (bump counter, publish). `nros::zephyr_component_main!(Talker)`
//! owns executor open, node registration, and the spin loop for this
//! self-package Rust application.
//! The user authors *only* the declarative + body bits.
//!
//! RMW selection still flows through the Kconfig `prj-<rmw>.conf`
//! overlay (vendor-native per L.12). The example `CMakeLists.txt`
//! threads the Kconfig `CONFIG_NROS_RMW_*` choice into Cargo feature
//! selection; `[package.metadata.nros.deploy.zephyr].rmw` is the
//! planner-side default when the Kconfig is unset.

#![no_std]

extern crate zephyr;

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker component — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

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

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            // Canonical talker fixture markers every Zephyr talker (c/cpp/rust)
            // emits — the E2E harness keys readiness off "Publishing messages"
            // (talker_created_pub) and counts "Published: <n>" lines
            // (published_count). The Rust publish path is silent (unlike the C
            // nros lib), so emit them here, mirroring the listener's "Received:"
            // line (issue #35: the zenoh native_sim rust pubsub failure was this
            // missing marker, not a transport fault).
            if *state == 0 {
                log::info!("Publishing messages");
            }
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 64>("/chatter", &msg);
            log::info!("Published: {}", *state);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);
nros::zephyr_component_main!(Talker);
