//! QEMU MPS2-AN385 RTIC Fibonacci Action Server — phase-244.D1 node logic.
//!
//! Serves an `example_interfaces/Fibonacci` action on `/fibonacci`. Declarative,
//! platform/RMW-agnostic Node: `register()` declares node + action server (goal /
//! cancel / accepted callbacks); `on_callback` runs the goal/cancel decisions;
//! `tick()` walks active goals, publishes feedback, completes them. The entry
//! crate's `nros::main!()` + the RTIC board (`nros-board-rtic-mps2-an385`) own
//! hardware/network bring-up, executor open, RMW registration, and the RTIC
//! dispatch loop. Locator/domain live in the entry's
//! `[package.metadata.nros.deploy.rtic-mps2-an385]` — never here.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, CancelResponse, ExecutableNode, GoalId, GoalResponse, GoalStatus, Node,
    NodeContext, NodeOptions, NodeResult, TickCtx,
};

/// Fibonacci action server — accepts non-negative goal orders and completes
/// each accepted goal with a canonical Fibonacci sequence.
pub struct FibonacciServer;

impl Node for FibonacciServer {
    const NAME: &'static str = "fibonacci_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_server"))?;
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
    /// Goals completed so far (informational).
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_goal" => {
                let response = match ctx.message::<FibonacciGoal>() {
                    Ok(goal) if goal.order >= 0 => GoalResponse::AcceptAndExecute,
                    _ => GoalResponse::Reject,
                };
                let _ = ctx.set_goal_response(response);
            }
            "on_cancel" => {
                let _ = ctx.set_cancel_response(CancelResponse::Ok);
            }
            "on_accepted" => {
                // No imperative work here — feedback/result are driven from
                // `tick()`, the only place the executor is free for action ops.
            }
            _ => {}
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        // Snapshot active goals into a fixed-cap stack list so the borrow-checker
        // lets us issue mutable executor ops after the visit returns.
        let mut pending: heapless::Vec<GoalId, 4> = heapless::Vec::new();
        ctx.for_each_active_goal_for_name("/fibonacci", &mut |goal_id, status| {
            if matches!(status, GoalStatus::Accepted | GoalStatus::Executing) {
                let _ = pending.push(*goal_id);
            }
        });

        for goal_id in pending {
            // The app-node shape doesn't surface the goal payload at tick time,
            // so emit a fixed order = 5 sequence incrementally as feedback, then
            // complete the goal.
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
                let _ = ctx.publish_feedback_for_name::<FibonacciFeedback, 256>(
                    "/fibonacci",
                    &goal_id,
                    &feedback,
                );
            }

            let result = FibonacciResult { sequence: seq };
            let _ = ctx.complete_goal_for_name::<FibonacciResult, 256>(
                "/fibonacci",
                &goal_id,
                GoalStatus::Succeeded,
                &result,
            );
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(FibonacciServer);
