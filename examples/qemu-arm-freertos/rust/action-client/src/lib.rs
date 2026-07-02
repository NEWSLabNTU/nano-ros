//! FreeRTOS QEMU MPS2-AN385 Fibonacci action client — declarative Node pkg.
//!
//! Declarative: node + action client.
//!
//! One-shot `send_goal` on the first `tick` call (after registration
//! completes). Feedback + result callbacks land via `on_callback` once
//! codegen wires the feedback-stream + result-subscriber +
//! `GoalStatusArray` topics through to dispatch; the demo-parity
//! wording for those transitions ("Goal accepted by server, waiting
//! for result" / "Next number in sequence received: [...]" /
//! "Result received: [...]") lands with that wiring.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
};

pub struct FibonacciClient;

impl Node for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client = node.create_action_client_for_name::<Fibonacci>("/fibonacci")?;
        Ok(())
    }
}

pub struct State {
    /// Set once the "Sending goal" line has been logged (first attempt).
    announced: bool,
    /// Set once the goal has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableNode for FibonacciClient {
    type State = State;

    fn init() -> Self::State {
        State {
            announced: false,
            sent: false,
        }
    }

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        // Feedback / result callbacks land here once codegen wires the
        // `GoalStatusArray` + feedback-stream + result-future
        // subscribers; this body is the seam.
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        if !state.announced {
            log::info!("Sending goal");
            state.announced = true;
        }
        let goal = FibonacciGoal { order: 10 };
        // 32 B is more than enough for one `i32` + CDR header.
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent = true;
        }
        // On send failure `sent` stays false — the next tick retries.
    }
}

nros::node!(FibonacciClient);
