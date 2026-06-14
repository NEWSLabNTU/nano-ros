//! QEMU MPS2-AN385 RTIC Fibonacci Action Client — phase-244.D1 node logic.
//!
//! Sends an `example_interfaces/Fibonacci` goal on `/fibonacci`. Declarative,
//! platform/RMW-agnostic Node: `register()` declares node + action client;
//! `tick()` issues a one-shot `send_goal` (then stays idempotent);
//! feedback/result callbacks land via `on_callback` once codegen wires the
//! result-future + feedback-stream + `GoalStatusArray` subscribers through to
//! dispatch. The entry crate's `nros::main!()` + the RTIC board
//! (`nros-board-rtic-mps2-an385`) own hardware/network bring-up, executor open,
//! RMW registration, and the RTIC dispatch loop. Locator/domain live in the
//! entry's `[package.metadata.nros.deploy.rtic-mps2-an385]` — never here.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
};

/// Fibonacci action client — declares the client, then issues a single goal
/// (`order = 5`) on the first `tick`.
pub struct FibonacciClient;

impl Node for FibonacciClient {
    const NAME: &'static str = "fibonacci_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_client"))?;
        let _client = node.create_action_client_for_name::<Fibonacci>("/fibonacci")?;
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

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        // Feedback / result callbacks land here once codegen wires the
        // `GoalStatusArray` + feedback-stream + result-future subscribers.
        // The id-driven dispatch is the M-F.4.a + N runtime plumbing; this
        // body is the seam.
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 5 };
        // 32 B is more than enough for one `i32` + CDR header.
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent = true;
        }
        // On a `Runtime` stub error, `sent` stays false — the next tick
        // retries. Once the real dispatch ships, the first successful send
        // flips the flag.
    }
}

nros::node!(FibonacciClient);
