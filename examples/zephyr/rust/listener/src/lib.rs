//! Zephyr Listener — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Subscribes to `std_msgs/String` on `/chatter` and logs each message
//! (`I heard: [Hello World: N]`), matching the official ROS 2
//! `demo_nodes_cpp` listener. `nros::zephyr_component_main!(Listener)` owns
//! executor open, node registration, and the spin loop for this
//! self-package Rust application.

#![no_std]

extern crate zephyr;

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::String as StringMsg;

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub =
            node.create_subscription_for_callback_name::<StringMsg>("on_chatter", "/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Number of messages seen on `/chatter` (state shared across ticks).
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if let Ok(msg) = ctx.message::<StringMsg>() {
                *state += 1;
                // Canonical delivery line every listener fixture (c/cpp/rust)
                // emits — the E2E `count_zephyr_received` asserts on
                // `I heard: [...]`. Without it the rust listener received
                // samples silently and the native→Zephyr E2E read 0 despite
                // working transport.
                log::info!("I heard: [{}]", msg.data);
            }
        }
    }
}

nros::node!(Listener);
nros::zephyr_component_main!(Listener);
