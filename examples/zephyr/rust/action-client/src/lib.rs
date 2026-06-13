//! Zephyr Fibonacci action client — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Declarative: node + action client.
//!
//! One-shot `send_goal` on the first `tick`; the terminal result is delivered
//! to `on_callback("on_result")` (phase-212 M-F.23 wires the single-node
//! runtime's action-client result dispatch).

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
                let len = ctx
                    .message::<FibonacciFeedback>()
                    .map(|f| f.sequence.len())
                    .unwrap_or(0);
                // Harness marker: client_got_feedback keys off "Feedback #".
                log::info!("Feedback #: sequence len={}", len);
            }
            "on_result" => {
                let n = ctx
                    .message::<FibonacciResult>()
                    .map(|r| r.sequence.len())
                    .unwrap_or(0);
                // Harness markers: "Action client finished" + "Result:".
                log::info!("Result: sequence len={}", n);
                log::info!("Action client finished");
            }
            _ => {}
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent = true;
            log::info!("Action client sent goal");
        }
    }
}

nros::node!(FibonacciClient);
nros::zephyr_component_main!(FibonacciClient);
