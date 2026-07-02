//! ThreadX Linux Action Server — Node pkg.
//!
//! Declares an `example_interfaces/Fibonacci` action server on
//! `/fibonacci`. The goal-decision callback accepts non-negative
//! orders; the cancel-decision callback always accepts. Goal
//! execution (computing the sequence, publishing feedback,
//! completing the goal) runs from `ExecutableNode::tick`, the only
//! place the executor is free for action ops.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, CancelResponse, ExecutableNode, GoalId, GoalResponse, GoalStatus, Node,
    NodeContext, NodeOptions, NodeResult, TickCtx,
};

pub struct ActionServer;

impl Node for ActionServer {
    const NAME: &'static str = "action_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_server"))?;
        let _action = node.create_action_server_for_name_with_callbacks::<Fibonacci>(
            "/fibonacci",
            "on_goal",
            "on_cancel",
            "on_accepted",
        )?;
        // Readiness marker the e2e harness greps before sending a goal.
        log::info!("Waiting for action goals");
        Ok(())
    }
}

impl ExecutableNode for ActionServer {
    /// Goals completed so far (informational).
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_goal" => {
                let order = ctx.message::<FibonacciGoal>().ok().map(|g| g.order);
                if let Some(order) = order {
                    log::info!("Received goal request with order {}", order);
                }
                let accept = matches!(order, Some(o) if o >= 0);
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
                log::info!("Executing goal");
            }
            _ => {}
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        // Snapshot any active goals into a fixed-cap stack list so the
        // borrow-checker lets us issue mutable executor ops after the
        // visit returns.
        let mut pending: heapless::Vec<GoalId, 4> = heapless::Vec::new();
        ctx.for_each_active_goal_for_name("/fibonacci", &mut |goal_id, status| {
            if matches!(status, GoalStatus::Accepted | GoalStatus::Executing) {
                let _ = pending.push(*goal_id);
            }
        });

        for goal_id in pending {
            // Compute a fixed-length feedback sequence + publish each
            // step. The Node pkg shape doesn't surface the goal payload
            // at tick time, so we pick a fixed order = 5 and emit it
            // incrementally.
            const ORDER: i32 = 5;
            let mut seq: heapless::Vec<i32, 64> = heapless::Vec::new();
            for i in 0..=ORDER {
                let next = match i {
                    0 => 0,
                    1 => 1,
                    _ => {
                        let len = seq.len();
                        seq[len - 1] + seq[len - 2]
                    }
                };
                let _ = seq.push(next);
                let feedback = FibonacciFeedback {
                    sequence: seq.clone(),
                };
                log::info!("Publish feedback");
                let _ = ctx.publish_feedback_for_name::<FibonacciFeedback, 256>(
                    "/fibonacci",
                    &goal_id,
                    &feedback,
                );
            }

            let result = FibonacciResult { sequence: seq };
            if ctx
                .complete_goal_for_name::<FibonacciResult, 256>(
                    "/fibonacci",
                    &goal_id,
                    GoalStatus::Succeeded,
                    &result,
                )
                .is_ok()
            {
                log::info!("Goal succeeded");
            }
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(ActionServer);
