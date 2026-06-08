//! ThreadX Linux Action Client — Phase 212.L Node pkg.
//!
//! Declares an `example_interfaces/Fibonacci` action client on
//! `/fibonacci`. Phase 212.M-F.4.b transcription: one-shot
//! `send_goal` on the first `tick` call. Feedback + result
//! callbacks land via `on_callback` once codegen wires the feedback-
//! stream + result-subscriber + `GoalStatusArray` topics through
//! to dispatch. The in-tree `UnsupportedClients` stub returns
//! `NodeDeclError::Runtime` until the M-F.4.a `GenClientDispatch`
//! reaches the installed nros-cli.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TickCtx,
};

pub struct ActionClient;

impl Node for ActionClient {
    const NAME: &'static str = "action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("cli_fib"), "/fibonacci")?;
        Ok(())
    }
}

pub struct State {
    /// Set once the goal has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableNode for ActionClient {
    type State = State;

    fn init() -> Self::State {
        State { sent: false }
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        if ctx
            .send_goal::<FibonacciGoal, 32>(EntityId::new("cli_fib"), &goal)
            .is_ok()
        {
            state.sent = true;
        }
    }
}

nros::node!(ActionClient);
