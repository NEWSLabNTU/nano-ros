//! Zephyr Fibonacci action server — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Declarative: node + action server with distinct goal / cancel /
//! accepted callbacks. Bodies:
//!  - `on_goal` accepts non-negative orders, rejects otherwise.
//!  - `on_cancel` always accepts.
//!  - `on_accepted` is a no-op (the per-spin work runs in `tick()`).
//!  - `tick()` walks every active goal, publishes feedback, completes.

#![no_std]

extern crate zephyr;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    CallbackCtx, CallbackId, CancelResponse, EntityId, ExecutableNode, GoalResponse, GoalStatus,
    Node, NodeContext, NodeOptions, NodeResult, TickCtx,
};

pub struct FibonacciServer;

impl Node for FibonacciServer {
    const NAME: &'static str = "fibonacci_action_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_server"))?;
        let _action = node.create_action_server_with_callbacks::<Fibonacci>(
            EntityId::new("act_fib"),
            CallbackId::new("on_goal"),
            CallbackId::new("on_cancel"),
            CallbackId::new("on_accepted"),
            "/fibonacci",
        )?;
        Ok(())
    }
}

impl ExecutableNode for FibonacciServer {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_goal" => {
                let accept = ctx
                    .message::<FibonacciGoal>()
                    .map(|g| g.order >= 0)
                    .unwrap_or(false);
                let _ = ctx.set_goal_response(if accept {
                    GoalResponse::AcceptAndExecute
                } else {
                    GoalResponse::Reject
                });
            }
            "on_cancel" => {
                let _ = ctx.set_cancel_response(CancelResponse::Ok);
            }
            "on_accepted" => {
                // No imperative work here; the executor drives feedback
                // and result through `tick()` when it is free for action ops.
            }
            _ => {}
        }
    }

    fn tick(_state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        // Collect goal ids to act on first; the typed feedback / result
        // calls borrow `ctx` mutably, so they can't run inside `visit`.
        let mut goals: nros::heapless::Vec<(nros::GoalId, i32), 4> = nros::heapless::Vec::new();
        ctx.for_each_active_goal(
            EntityId::new("act_fib"),
            &mut |goal_id, _status: GoalStatus| {
                let _ = goals.push((*goal_id, 0));
            },
        );

        for (goal_id, _order) in goals {
            // Publish one canonical Fibonacci-shaped feedback frame.
            let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();
            let _ = sequence.push(0);
            let _ = sequence.push(1);
            let _ = sequence.push(1);
            let feedback = FibonacciFeedback {
                sequence: sequence.clone(),
            };
            let _ = ctx.publish_feedback::<FibonacciFeedback, 128>(
                EntityId::new("act_fib"),
                &goal_id,
                &feedback,
            );

            let result = FibonacciResult { sequence };
            let _ = ctx.complete_goal::<FibonacciResult, 128>(
                EntityId::new("act_fib"),
                &goal_id,
                GoalStatus::Succeeded,
                &result,
            );
        }
    }
}

nros::node!(FibonacciServer);
nros::zephyr_component_main!(FibonacciServer);
