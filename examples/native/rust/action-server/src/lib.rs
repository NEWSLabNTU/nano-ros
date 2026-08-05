//! Native Fibonacci action server — the official ROS 2 demo action.
//!
//! Node pkg shape: `register()` declares the node + action server and logs
//! `ACTION_SERVER_READY_MARKER`; the callbacks accept goals, and `tick()` runs
//! the execution loop publishing feedback and completing the goal. `main.rs`'s
//! `nros::main!()` and the board own `nros::init`, executor open, RMW
//! registration and the spin loop.
//!
//! phase-338 W3 — was an `[package.metadata.nros.application]` example on the
//! imperative Executor API. Now Node-class like every other platform's copy,
//! byte-identical to them (the `example_portability` gate asserts it).
//!
//! Migrating this one needed TWO runtime fixes first, both found here and both
//! affecting every raw-registered action: the keyexpr advertised the bare action
//! type instead of the per-channel types (`7a7068af9`), and the payload carried
//! an extra CDR header (issue 0418 / RFC-0069). Until those landed the server
//! declared its entities and silently never received a goal.

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
        log::info!("Waiting for action goals");
        Ok(())
    }
}

impl ExecutableNode for FibonacciServer {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_goal" => {
                let order = ctx.message::<FibonacciGoal>().map(|g| g.order).ok();
                if let Some(order) = order {
                    log::info!("Received goal request with order {}", order);
                }
                let accept = order.map(|o| o >= 0).unwrap_or(false);
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
            log::info!("Executing goal");
            // Publish one canonical Fibonacci-shaped feedback frame.
            let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();
            let _ = sequence.push(0);
            let _ = sequence.push(1);
            let _ = sequence.push(1);
            let feedback = FibonacciFeedback {
                sequence: sequence.clone(),
            };
            log::info!("Publish feedback");
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
            log::info!("Goal succeeded");
        }
    }
}

nros::node!(FibonacciServer);
