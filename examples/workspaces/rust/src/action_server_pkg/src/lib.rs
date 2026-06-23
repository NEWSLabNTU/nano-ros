//! ActionServer Node pkg — serves `example_interfaces/action/Fibonacci` on
//! `/fibonacci` (phase-263 A4).
//!
//! Declarative shape: `register()` declares an action SERVER with distinct
//! goal/cancel/accepted callback ids; `on_callback("cb_fib_goal")` accepts the goal
//! (`GoalResponse::AcceptAndExecute`); `tick(TickCtx)` drives the active goal to
//! completion — grows the Fibonacci sequence, publishes feedback, and completes with
//! `GoalStatus::Succeeded` once the sequence reaches 11 elements. Mirrors the
//! tick-driven server pattern from the orchestration test's `fib_server`.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};
use nros::{
    Callback, CallbackCtx, ExecutableNode, GoalResponse, GoalStatus, Node, NodeContext,
    NodeOptions, NodeResult, TickCtx,
};

pub struct ActionServer;

impl Node for ActionServer {
    const NAME: &'static str = "fibonacci_action_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_server"))?;
        let _act = node.create_action_server_for_name_with_callbacks::<Fibonacci>(
            "/fibonacci",
            "cb_fib_goal",
            "cb_fib_cancel",
            "cb_fib_accepted",
        )?;
        Ok(())
    }
}

impl ExecutableNode for ActionServer {
    /// Ticks since the active goal appeared — drives the sequence length.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "cb_fib_goal" {
            // Accept + execute; `tick` drives feedback + result.
            let _ = ctx.set_goal_response(GoalResponse::AcceptAndExecute);
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        let mut goal: Option<nros::GoalId> = None;
        ctx.for_each_active_goal_for_name("/fibonacci", &mut |g, _status| {
            if goal.is_none() {
                goal = Some(*g);
            }
        });
        let Some(goal_id) = goal else {
            return;
        };
        *state = state.wrapping_add(1);
        let n = (*state as usize).min(11);
        let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();
        let (mut a, mut b) = (0i32, 1i32);
        for _ in 0..n {
            let _ = sequence.push(a);
            let next = a + b;
            a = b;
            b = next;
        }
        let feedback = FibonacciFeedback {
            sequence: sequence.clone(),
        };
        let _ = ctx.publish_feedback_for_name::<FibonacciFeedback, 512>(
            "/fibonacci",
            &goal_id,
            &feedback,
        );
        if n >= 11 {
            let result = FibonacciResult { sequence };
            let _ = ctx.complete_goal_for_name::<FibonacciResult, 512>(
                "/fibonacci",
                &goal_id,
                GoalStatus::Succeeded,
                &result,
            );
            *state = 0;
        }
    }
}

nros::node!(ActionServer);
