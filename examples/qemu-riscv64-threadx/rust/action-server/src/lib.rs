//! ThreadX QEMU RISC-V Fibonacci Action Server — app-node logic.
//!
//! Serves an `example_interfaces/Fibonacci` action on `/fibonacci`. This is an
//! **app node** (it owns `main`, via `src/main.rs`'s `nros::main!()`), not a
//! workspace Node lib — but the *logic* is still platform/RMW-agnostic:
//! `register()` declares node + action server (goal / cancel / accepted
//! callbacks); `on_callback` runs the goal/cancel decisions; `tick()` walks
//! active goals, publishes feedback, and completes them. The board
//! (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns `nros::init`,
//! executor open, RMW registration, and the spin loop. RMW selection
//! (zenoh / cyclonedds) lives in `Cargo.toml [features]`; the locator + domain
//! in `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.

#![no_std]

mod app_main;

// Keep the board crate (panic handler + allocator + critical-section impl)
// linked into the standalone `staticlib` even on the zenoh/cargo path, where
// only `main.rs`'s `nros::main!()` names it (issue #205 — the per-example
// critical-section anchor moved into the board crate).

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

// CycloneDDS / CMake firmware path: the C `startup.c::main` calls
// `tx_kernel_enter()` and dispatches to this `app_main` *inside* the ThreadX app
// thread — so the kernel is already running here. `run_app_thread` runs the
// post-kernel body (open executor + `register` + spin); it must NOT re-enter the
// kernel via `BoardEntry::run`. The zenoh/cargo path uses `src/main.rs`'s
// `nros::main!()` instead and never compiles this. Both are thin — the board owns
// executor open, RMW registration, and the spin loop; the `nros::node!()`-emitted
// `register` declares the FibonacciServer. No manual `Executor::open` /
// `register_rmw` / spin loop / hardcoded locator in the example.
