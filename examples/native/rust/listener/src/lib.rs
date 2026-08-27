//! Native Listener — the official ROS 2 `demo_nodes_cpp` listener.
//!
//! Subscribes `std_msgs/String` on `/chatter` and prints
//! `I heard: [Hello World: N]`.
//!
//! Node pkg shape: `register()` declares the node + subscription and logs the
//! readiness marker; `on_callback("on_chatter")` handles each message.
//! `main.rs`'s `nros::main!()` and the board own `nros::init`, executor open,
//! RMW registration and the spin loop.
//!
//! phase-338 W3 — was an `[package.metadata.nros.application]` example written
//! against the imperative Executor API. Now Node-class like every other
//! platform's copy, byte-identical to them (the `example_portability` gate
//! asserts it). The "Subscriber created" readiness line `native_api.rs` gates
//! on was added to every group copy so this one did not have to diverge.

#![no_std]

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::String as StringMsg;

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    // phase-391 W5-endgame (issue 0857) — exact bounds: one subscription,
    // no publishers/services/actions, so the static cell pays ~nothing.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

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
