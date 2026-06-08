//! NuttX QEMU ARM Fibonacci action client — Phase 212.L Node pkg.
//!
//! Declarative: node + action client.
//!
//! Phase 212.M-F.4.b transcription: one-shot `send_goal` on the first
//! `tick` call (after registration completes). Feedback + result
//! callbacks land via `on_callback` once codegen wires the feedback-
//! stream + result-subscriber + `GoalStatusArray` topics through to
//! dispatch. The in-tree `UnsupportedClients` stub returns
//! `NodeDeclError::Runtime` from `send_goal_raw` until the M-F.4.a
//! `GenClientDispatch` reaches the installed nros-cli.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TickCtx,
};

pub struct FibonacciClient;

impl Node for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("client_fib"), "/fibonacci")?;
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

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // Feedback / result callbacks land here once codegen wires the
        // `GoalStatusArray` + feedback-stream + result-future
        // subscribers. The id-driven dispatch is the M-F.4.a + N
        // runtime plumbing; this body is the seam.
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        // 32 B is more than enough for one `i32` + CDR header.
        if ctx
            .send_goal::<FibonacciGoal, 32>(EntityId::new("client_fib"), &goal)
            .is_ok()
        {
            state.sent = true;
        }
        // On `Err(NodeDeclError::Runtime)` (today's stub), `sent`
        // stays false — the next tick retries. Once M-F.4.a ships the
        // real dispatch, the first successful send flips the flag.
    }
}

nros::node!(FibonacciClient);
