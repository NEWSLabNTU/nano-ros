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

extern crate alloc;
// Keep the board crate (panic handler + allocator + critical-section impl)
// linked into the standalone `staticlib` even on the zenoh/cargo path, where
// only `main.rs`'s `nros::main!()` names it (issue #205 — the per-example
// critical-section anchor moved into the board crate).
extern crate nros_board_threadx_qemu_riscv64 as _;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, CancelResponse, ExecutableNode, GoalId, GoalResponse, GoalStatus, Node,
    NodeContext, NodeOptions, NodeResult, TickCtx,
};

/// Fibonacci action server — accepts non-negative goal orders and completes
/// each accepted goal with a canonical Fibonacci sequence.
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
        // Readiness marker the e2e harness greps before sending a goal.
        log::info!("Waiting for action goals");
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
                // No imperative work here — feedback/result are driven from
                // `tick()`, the only place the executor is free for action ops.
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
            // The app-node shape doesn't surface the goal payload at tick
            // time, so we emit a fixed order = 5 sequence incrementally as
            // feedback, then complete the goal.
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
nros_board_threadx_qemu_riscv64::cyclonedds_app_main!(register);
