//! Zephyr Talker — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Publishes `std_msgs/String` (`Hello World: N`) on `/chatter` once per
//! second, matching the official ROS 2 `demo_nodes_cpp` talker.
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

mod app_main;


use core::fmt::Write as _;

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::String as StringMsg;

/// Talker component — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    // phase-391 W5-endgame (issue 0857) — exact bounds: one publisher.
    // The per-class cell is a STATIC sized by these; the knob default
    // (8 slots x ~1.35 KiB) collapsed the esp32 single-image stack.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let pub_chatter = node.create_publisher_for_topic::<StringMsg>("/chatter")?;
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
            // Canonical chatter line every talker (c/cpp/rust, all platforms)
            // emits — the E2E harness keys readiness off the first
            // `Publishing:` line (TALKER_READY_MARKER) and counts them
            // (published_count). The Rust publish path is silent (unlike the
            // C nros lib), so emit it here, mirroring the listener's
            // `I heard:` line (issue #35: the zenoh native_sim rust pubsub
            // failure was this missing marker, not a transport fault).
            *state = state.wrapping_add(1);
            let mut msg = StringMsg::default();
            let _ = write!(msg.data, "Hello World: {}", *state);
            let _ = ctx.publish_to_topic::<StringMsg, 64>("/chatter", &msg);
            log::info!("Publishing: '{}'", msg.data);
        }
    }
}

nros::node!(Talker);
// Issue 0330 — force-link the selected RMW backend into this staticlib. The
// facade macro below used to emit these references itself, which named two
// concrete backends in an RMW-agnostic layer; naming one here is correct,
// because selecting an RMW is exactly what this crate does. Registration is
// still done by `nros_app_register_backends` — this is only a DCE anchor
// (issues 0155 / 0163). cyclonedds needs none: its register entry lives in the
// Zephyr module's C++ lib, which the image already links.

