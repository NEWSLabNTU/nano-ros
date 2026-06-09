//! Zephyr Listener — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and tracks the last seen
//! value. `nros::zephyr_component_main!(Listener)` owns executor open,
//! node registration, and the spin loop for this self-package Rust
//! application.

#![no_std]

extern crate zephyr;

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::Int32;

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub = node.create_subscription_for_callback_name::<Int32>("on_chatter", "/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Last value seen on `/chatter` (state shared across callback ticks).
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if let Ok(msg) = ctx.message::<Int32>() {
                *state = msg.data;
                // Canonical delivery line every Zephyr listener fixture
                // (c/cpp/rust) emits — the E2E `count_zephyr_received`
                // asserts on `Received: <n>`. Without it the rust
                // listener received samples silently and the
                // native→Zephyr E2E read 0 despite working transport.
                log::info!("Received: {}", msg.data);
            }
        }
    }
}

nros::node!(Listener);
nros::zephyr_component_main!(Listener);
