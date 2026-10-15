//! Native Fibonacci action client — the official ROS 2 demo action client.
//!
//! Sends one goal, reports each feedback sample and the final result.
//! `main.rs`'s `nros::main!()` and the board own `nros::init`, executor open,
//! RMW registration and the spin loop.
//!
//! phase-338 W3 — was an `[package.metadata.nros.application]` example on the
//! imperative Executor API. Now Node-class like every other platform's copy.
//!
//! Output: "Waiting for action server..." (once, until the server is
//! discoverable), "Sending goal", "Goal sent, waiting for result", "Next number
//! in sequence received", "Result received". It says SENT, not "accepted": the
//! declarative `send_goal_for_name` does not wait for the server's answer, so
//! the old "Goal accepted by server" line claimed something this client never
//! saw (issue 1918). The C and C++ clients, whose `send_goal` does wait, still
//! print the accepted line. The imperative
//! version's extra ERROR-path lines ("Failed to create action client", "Goal
//! was never accepted", …) are asserted by no Rust test; the one rejection
//! assertion that exists drives the C++ client, not this one.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TickCtx,
};

pub struct FibonacciClient;

impl Component for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 1, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client = node.create_action_client_with_callbacks_for_name::<Fibonacci>(
            "/fibonacci",
            "on_result",
            "on_feedback",
        )?;
        Ok(())
    }
}

pub struct State {
    /// Set once the goal has been sent — keeps `tick` idempotent.
    sent: bool,
    /// Issue 1918 — set once "waiting for the action server" has been logged.
    waiting_logged: bool,
}

impl ExecutableNode for FibonacciClient {
    type State = State;

    fn init() -> Self::State {
        State {
            sent: false,
            waiting_logged: false,
        }
    }

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_feedback" => {
                if let Ok(f) = ctx.message::<FibonacciFeedback>() {
                    log::info!("Next number in sequence received: {:?}", f.sequence);
                }
            }
            "on_result" => {
                if let Ok(r) = ctx.message::<FibonacciResult>() {
                    log::info!("Result received: {:?}", r.sequence);
                }
            }
            _ => {}
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        // Issue 1918 — wait for the server before the ONE send. A goal sent
        // before the server has declared its send_goal service is answered by
        // nobody, and nothing re-sends it. `Err` means the backend cannot say
        // (no discovery channel), so send anyway.
        if let Ok(false) = ctx.action_server_is_ready_for_name("/fibonacci") {
            if !state.waiting_logged {
                log::info!("Waiting for action server...");
                state.waiting_logged = true;
            }
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        log::info!("Sending goal");
        // 32 B is more than enough for one `i32` + CDR header.
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent = true;
            // Issue 1918 — SENT, not accepted: `send_goal_for_name` does not
            // wait for the server's answer, so this cannot claim one.
            log::info!("Goal sent, waiting for result");
        }
        // On send failure `sent` stays false — the next tick retries.
    }
}

nros::node!(FibonacciClient);
