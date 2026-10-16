//! FibonacciClient Node pkg — calls the `example_interfaces/action/Fibonacci`
//! server on `/fibonacci` once, then republishes the final Fibonacci number on
//! `/fib_result`.
//!
//! Declarative shape with BOTH dispatch surfaces (phase-263 A4, the first
//! workspace example to exercise the action-CLIENT path):
//!   - `register()` declares an action CLIENT (result + feedback callbacks) and a
//!     `/fib_result` publisher.
//!   - `tick(TickCtx)` sends the goal exactly once (gated by a `sent` flag) — goal
//!     send is a tick-only op, the executor is free there.
//!   - `on_callback("on_feedback")` observes the streamed partial sequence;
//!     `on_callback("on_result")` reads the terminal result and publishes its last
//!     element on `/fib_result`. Publishing (not logging) keeps the result
//!     observable on the wire — the workspace shape inits no log sink yet (A5).

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TickCtx,
};
use std_msgs::msg::Int32;

/// Fibonacci action client — one-shot goal, result republished on `/fib_result`.
pub struct FibonacciClient;

/// How long a goal sent BLIND (the backend cannot see servers — XRCE, issue
/// 1686) may go without a result before it is sent again.
const BLIND_RESEND: core::time::Duration = core::time::Duration::from_secs(5);

/// When the goal was last sent, whether that send was blind, and whether a
/// result has arrived (`examples/linux/rust/action-client` says why).
pub struct ClientState {
    sent_at: Option<core::time::Duration>,
    blind: bool,
    done: bool,
}

impl Component for FibonacciClient {
    const NAME: &'static str = "fibonacci_client";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 1, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_client"))?;
        let _client = node.create_action_client_with_callbacks_for_name::<Fibonacci>(
            "/fibonacci",
            "on_result",
            "on_feedback",
        )?;
        let pub_result = node.create_publisher_for_topic::<Int32>("/fib_result")?;
        node.callback_for_name("on_result")
            .publishes_entity(&pub_result)?;
        Ok(())
    }
}

impl ExecutableNode for FibonacciClient {
    type State = ClientState;

    fn init() -> Self::State {
        ClientState {
            sent_at: None,
            blind: false,
            done: false,
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_feedback" => {
                // Partial sequence streamed mid-goal — observed, nothing to emit.
                let _ = ctx.message::<FibonacciFeedback>();
            }
            "on_result" => {
                if let Ok(result) = ctx.message::<FibonacciResult>() {
                    state.done = true;
                    let last = result.sequence.last().copied().unwrap_or(0);
                    let msg = Int32 { data: last };
                    let _ = ctx.publish_to_topic::<Int32, 8>("/fib_result", &msg);
                }
            }
            _ => {}
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.done {
            return;
        }
        let now = nros::time::now();
        // A confirmed send is never repeated; a BLIND one (the backend cannot
        // see servers) is, after `BLIND_RESEND` of silence: a goal written
        // before XRCE's Agent matched the two endpoints is dropped, and nothing
        // else would ever re-send it.
        if let Some(at) = state.sent_at
            && (!state.blind || now.saturating_sub(at) < BLIND_RESEND)
        {
            return;
        }
        // Issue 1918 — wait for the server before sending.
        let blind = match ctx.action_server_is_ready_for_name("/fibonacci") {
            Ok(false) => return,
            Ok(true) => false,
            Err(_) => true,
        };
        let goal = FibonacciGoal { order: 10 };
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent_at = Some(now);
            state.blind = blind;
        }
    }
}

nros::node!(FibonacciClient);
