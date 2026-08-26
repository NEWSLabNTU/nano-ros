//! Zephyr Listener — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Subscribes to `std_msgs/String` on `/chatter` and logs each message
//! (`I heard: [Hello World: N]`), matching the official ROS 2
//! `demo_nodes_cpp` listener. `nros::zephyr_component_main!(Listener)` owns
//! executor open, node registration, and the spin loop for this
//! self-package Rust application.

#![no_std]

mod app_main;

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::String as StringMsg;

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub =
            node.create_subscription_for_callback_name::<StringMsg>("on_chatter", "/chatter")?;
        // Shared readiness marker every nano-ros listener prints
        // (`nros_tests::output::LISTENER_READY_MARKER`, phase-342 W7). This
        // component had none: the zephyr rust cells waited on the ENTRY's boot
        // banner instead, so "the subscription exists" was never observable.
        log::info!("Subscriber created for topic: /chatter");
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
        // Two constraints meet here, and only this shape satisfies both: this
        // crate is edition 2021, where a LET-CHAIN is a hard error, and rust
        // 1.97's clippy denies the nested `if` the chain replaced. Early
        // returns need neither. (`8e7307c99` collapsed the nesting for clippy
        // and broke the build for everyone on the west lane, which is the only
        // lane that compiles this leaf.)
        if callback.as_str() != "on_chatter" {
            return;
        }
        let Ok(msg) = ctx.message::<StringMsg>() else {
            return;
        };
        *state += 1;
        // Canonical delivery line every listener fixture (c/cpp/rust) emits —
        // the E2E `count_zephyr_received` asserts on `I heard: [...]`. Without
        // it the rust listener received samples silently and the native→Zephyr
        // E2E read 0 despite working transport.
        log::info!("I heard: [{}]", msg.data);
    }
}

nros::node!(Listener);
// Issue 0330 — force-link the selected RMW backend into this staticlib. The
// facade macro below used to emit these references itself, which named two
// concrete backends in an RMW-agnostic layer; naming one here is correct,
// because selecting an RMW is exactly what this crate does. Registration is
// still done by `nros_app_register_backends` — this is only a DCE anchor
// (issues 0155 / 0163). cyclonedds needs none: its register entry lives in the
// Zephyr module's C++ lib, which the image already links.
