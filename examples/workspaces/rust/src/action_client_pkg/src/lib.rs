//! ActionClient Node pkg — calls `example_interfaces/action/Fibonacci` on
//! `/fibonacci` and republishes the result's last sequence element on `/fib_result`
//! (phase-263 A4).
//!
//! Declarative shape with both dispatch surfaces:
//!   - `register()` declares an action CLIENT bound to a result + feedback callback,
//!     plus a `/fib_result` publisher.
//!   - `tick(TickCtx)` sends ONE goal (`order = 10`) once the server is reachable;
//!     `send_goal` is retried each tick until it succeeds (the server is a separate
//!     process and may not be discovered on the first tick).
//!   - `on_callback("cb_fib_result")` fires when the executor auto-delivers the goal
//!     result; it reads the `FibonacciResult` and publishes its last sequence element
//!     (the largest Fibonacci number) on `/fib_result`.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
};
use std_msgs::msg::Int32;

pub struct ActionClient;

/// Whether the single goal has been accepted by the server yet.
pub struct ClientState {
    sent: bool,
}

impl Node for ActionClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client = node.create_action_client_with_callbacks_for_name::<Fibonacci>(
            "/fibonacci",
            "cb_fib_result",
            "cb_fib_feedback",
        )?;
        let pub_result = node.create_publisher_for_topic::<Int32>("/fib_result")?;
        node.callback_for_name("cb_fib_result")
            .publishes_entity(&pub_result)?;
        Ok(())
    }
}

impl ExecutableNode for ActionClient {
    type State = ClientState;

    fn init() -> Self::State {
        ClientState { sent: false }
    }

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "cb_fib_result"
            && let Ok(result) = ctx.message::<FibonacciResult>()
        {
            // Last element = the largest computed Fibonacci number.
            let last = result.sequence.last().copied().unwrap_or(0);
            let msg = Int32 { data: last };
            let _ = ctx.publish_to_topic::<Int32, 8>("/fib_result", &msg);
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        // 16-byte goal CDR (one int32 + header). Retry until the cross-process server
        // is discovered and accepts the request.
        if ctx
            .send_goal_for_name::<FibonacciGoal, 16>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent = true;
        }
    }
}

nros::node!(ActionClient);
