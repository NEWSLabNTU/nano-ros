//! FreeRTOS QEMU MPS2-AN385 Listener — Phase 212.L Node pkg.
//!
//! Subscribes to `std_msgs/String` on `/chatter` and logs each message
//! (`I heard: [Hello World: N]`). The BSP-generated runtime owns init / executor / spin.

#![no_std]

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::String as StringMsg;

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub =
            node.create_subscription_for_callback_name::<StringMsg>("on_chatter", "/chatter")?;
        // phase-338 W3 — readiness marker. `native_api.rs` gates on
        // "Subscriber created" (`.expect("rust-listener did not become ready")`),
        // and the native listener could not become Node-class while only it
        // emitted the line. Additive for embedded, whose readiness comes from
        // the board banner; mirrors the group service-server, which already
        // logs its own readiness inside `register`.
        log::info!("Subscriber created for topic: /chatter");
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Number of messages seen on `/chatter`.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter"
            && let Ok(msg) = ctx.message::<StringMsg>()
        {
            *state = state.wrapping_add(1);
            // Canonical delivery line (phase-277 W4) — the rtos e2e
            // harness counts `I heard:` lines; without it a working
            // listener looked silent (pre-existing gap found in T4).
            log::info!("I heard: [{}]", msg.data);
        }
    }
}

nros::node!(Listener);
