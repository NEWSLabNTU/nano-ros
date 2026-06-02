//! ThreadX Linux Action Server — Phase 212.L Component pkg.
//!
//! Declares an `example_interfaces/Fibonacci` action server on
//! `/fibonacci`. The goal-decision callback accepts non-negative
//! orders; the cancel-decision callback always accepts. Goal
//! execution (computing the sequence, publishing feedback,
//! completing the goal) runs from `ExecutableComponent::tick`
//! (W.5.6 — needs the executor, hence tick-only).

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    CallbackCtx, CallbackId, CancelResponse, Component, ComponentContext, ComponentResult,
    EntityId, ExecutableComponent, GoalId, GoalResponse, GoalStatus, NodeId, NodeOptions, TickCtx,
};

pub struct ActionServer;

impl Component for ActionServer {
    const NAME: &'static str = "action_server";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(
            NodeId::new("node"),
            NodeOptions::new("fibonacci_action_server"),
        )?;
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

impl ExecutableComponent for ActionServer {
    /// Goals completed so far (informational).
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(_state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
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
            _ => {}
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        // Snapshot any active goals into a fixed-cap stack list so the
        // borrow-checker lets us issue mutable executor ops after the
        // visit returns.
        let mut pending: heapless::Vec<GoalId, 4> = heapless::Vec::new();
        ctx.for_each_active_goal(EntityId::new("act_fib"), &mut |goal_id, status| {
            if matches!(status, GoalStatus::Accepted | GoalStatus::Executing) {
                let _ = pending.push(*goal_id);
            }
        });

        for goal_id in pending {
            // Compute a fixed-length feedback sequence + publish each
            // step. The Component pkg shape doesn't surface the goal
            // payload at tick time (W.5.6 minimum), so we pick a fixed
            // order = 5 and emit it incrementally.
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
                let _ = ctx.publish_feedback::<FibonacciFeedback, 256>(
                    EntityId::new("act_fib"),
                    &goal_id,
                    &feedback,
                );
            }

            let result = FibonacciResult { sequence: seq };
            let _ = ctx.complete_goal::<FibonacciResult, 256>(
                EntityId::new("act_fib"),
                &goal_id,
                GoalStatus::Succeeded,
                &result,
            );
            *state = state.wrapping_add(1);
        }
    }
}

nros::component!(ActionServer);

/// Phase 212.N.7 step-2 — Entry-pkg-callable wrapper.
///
/// The codegen-emitted `run_plan(runtime)` body (see
/// `nros-build::generate_run_plan`, §212.N.4) dispatches one
/// `<pkg>::register(runtime)?` call per launch-XML `<node>` entry.
/// This wrapper is the stable per-Component-pkg API the Entry pkg
/// links against — board-agnostic, no `nros::init` / executor /
/// spin (those live in `BoardEntry::run`).
///
/// Today the wrapper is a stub: the per-component declarative
/// registration still flows through the `nros::component!`-emitted
/// trampoline that `Executor::add_components` invokes after
/// `BoardEntry::run` opens the executor. Once §212.N.4 codegen
/// lands the full `RuntimeCtx`-aware launch overlay, this body
/// will bridge `runtime` into the component's `ComponentContext`.
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
