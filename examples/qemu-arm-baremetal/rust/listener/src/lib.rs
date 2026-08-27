//! Node pkg for the QEMU MPS2-AN385 bare-metal listener.
//!
//! Platform/RMW-agnostic application logic (RFC-0024 shape): declares a
//! subscription on `/chatter` bound to the `on_message` callback; each typed
//! `std_msgs/String` delivery logs `I heard: [Hello World: N]`. The boot scaffold
//! (reset → `BoardEntry::run_with_deploy` → executor → spin) is owned by
//! `nros::main!()` + `nros-board-mps2-an385` (Phase 244.D1 enabler) — none of
//! it appears here. The old imperative `run(Config, |cfg| { … })` closure
//! (executor open, RMW register, manual spin loop) folds into the boot
//! scaffold; only the declarative node survives.

#![no_std]

use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions,
    NodeResult, TickCtx,
};
use std_msgs::msg::String as StringMsg;

// Phase 88.16.C — diagnostics route through `nros-log`.
pub struct ListenerNode;

impl Node for ListenerNode {
    const NAME: &'static str = "listener";

    // phase-391 W5-endgame (issue 0857) — exact bounds: one subscription,
    // no publishers/services/actions, so the static cell pays ~nothing.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        node.create_subscription_for_callback_name::<StringMsg>("on_message", "/chatter")?;
        log::info!("Subscribing to /chatter (std_msgs/String)");
        // phase-342 W7 — the READINESS marker the harness waits on
        // (`output::LISTENER_READY_MARKER`, via `expect_ready`). Every listener
        // prints this same line; "Subscriber declared" below is kept because
        // the baremetal QEMU logs are read by eye as well as by grep.
        log::info!("Subscriber created for topic: /chatter");
        log::info!("Subscriber declared");
        log::info!("Waiting for messages...");
        Ok(())
    }
}

impl ExecutableNode for ListenerNode {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut (), callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_message"
            && let Ok(msg) = ctx.message::<StringMsg>()
        {
            log::info!("I heard: [{}]", msg.data);
        }
    }

    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

nros::node!(ListenerNode);
