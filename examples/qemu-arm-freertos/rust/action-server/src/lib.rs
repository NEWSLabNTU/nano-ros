//! FreeRTOS QEMU MPS2-AN385 Fibonacci action server —
//! Phase 212.L Node pkg.
//!
//! Declarative: node + action server with distinct goal / cancel /
//! accepted callbacks. Bodies:
//!  - `on_goal` accepts non-negative orders, rejects otherwise.
//!  - `on_cancel` always accepts.
//!  - `on_accepted` is a no-op (per-spin work runs in `tick()`).
//!  - `tick()` walks every active goal, publishes feedback, completes.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, CancelResponse, ExecutableNode, GoalResponse, GoalStatus, Node,
    NodeContext, NodeOptions, NodeResult, TickCtx,
};

pub struct FibonacciServer;

impl Node for FibonacciServer {
    const NAME: &'static str = "fibonacci_action_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_server"))?;
        let _action = node.create_action_server_for_name_with_callbacks::<Fibonacci>(
            "/fibonacci",
            "on_goal",
            "on_cancel",
            "on_accepted",
        )?;
        Ok(())
    }
}

impl ExecutableNode for FibonacciServer {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
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
                // Per-spin work runs in `tick()` (the only place the
                // executor is free for action ops).
            }
            _ => {}
        }
    }

    fn tick(_state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        // Collect goal ids first — typed feedback / result calls borrow
        // `ctx` mutably so they can't run inside `visit`.
        let mut goals: nros::heapless::Vec<(nros::GoalId, i32), 4> = nros::heapless::Vec::new();
        ctx.for_each_active_goal_for_name("/fibonacci", &mut |goal_id, _status: GoalStatus| {
            let _ = goals.push((*goal_id, 0));
        });

        for (goal_id, _order) in goals {
            // Publish one canonical Fibonacci-shaped feedback frame.
            let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();
            let _ = sequence.push(0);
            let _ = sequence.push(1);
            let _ = sequence.push(1);
            let feedback = FibonacciFeedback {
                sequence: sequence.clone(),
            };
            let _ = ctx.publish_feedback_for_name::<FibonacciFeedback, 128>(
                "/fibonacci",
                &goal_id,
                &feedback,
            );

            let result = FibonacciResult { sequence };
            let _ = ctx.complete_goal_for_name::<FibonacciResult, 128>(
                "/fibonacci",
                &goal_id,
                GoalStatus::Succeeded,
                &result,
            );
        }
    }
}

nros::node!(FibonacciServer);
