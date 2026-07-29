//! Zephyr Fibonacci action client.
//!
//! Declarative: node + action client.
//!
//! One-shot `send_goal` on the first `tick`; feedback and the terminal
//! result are delivered to `on_callback` (`on_feedback` / `on_result`).

#![no_std]

extern crate zephyr;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
};

pub struct FibonacciClient;

impl Node for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client = node.create_action_client_with_callbacks_for_name::<Fibonacci>(
            "/fibonacci",
            "on_result",
            "on_feedback",
        )?;
        Ok(())
    }
}

pub struct State {
    /// Set once the goal has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableNode for FibonacciClient {
    type State = State;

    fn init() -> Self::State {
        State { sent: false }
    }

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_feedback" => {
                if let Ok(f) = ctx.message::<FibonacciFeedback>() {
                    log::info!("Next number in sequence received: {:?}", f.sequence);
                }
            }
            "on_result" => {
                if let Ok(r) = ctx.message::<FibonacciResult>() {
                    log::info!("Result received: {:?}", r.sequence);
                }
            }
            _ => {}
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        log::info!("Sending goal");
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent = true;
            log::info!("Goal accepted by server, waiting for result");
        }
    }
}

nros::node!(FibonacciClient);
// Issue 0330 — force-link the selected RMW backend into this staticlib. The
// facade macro below used to emit these references itself, which named two
// concrete backends in an RMW-agnostic layer; naming one here is correct,
// because selecting an RMW is exactly what this crate does. Registration is
// still done by `nros_app_register_backends` — this is only a DCE anchor
// (issues 0155 / 0163). cyclonedds needs none: its register entry lives in the
// Zephyr module's C++ lib, which the image already links.
#[cfg(feature = "rmw-zenoh")]
nros::force_link_backend!(nros_rmw_zenoh);
#[cfg(feature = "rmw-xrce")]
nros::force_link_backend!(nros_rmw_xrce_cffi);

nros::zephyr_component_main!(FibonacciClient);
