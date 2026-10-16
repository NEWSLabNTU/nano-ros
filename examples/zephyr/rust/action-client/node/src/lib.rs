//! Zephyr Fibonacci action client.
//!
//! Declarative: node + action client.
//!
//! One-shot `send_goal` on the first `tick`; feedback and the terminal
//! result are delivered to `on_callback` (`on_feedback` / `on_result`).

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

/// How long a goal sent BLIND (the backend cannot see servers) may go without
/// a result before it is sent again. The demo server finishes order 10 in a
/// few milliseconds, so five seconds is far past any real answer.
const BLIND_RESEND: core::time::Duration = core::time::Duration::from_secs(5);

pub struct State {
    /// When the goal was last sent; `None` until the first send.
    sent_at: Option<core::time::Duration>,
    /// Whether that send was BLIND — the backend could not say whether a
    /// server exists (`action_server_is_ready` → `Err`, XRCE: the Agent owns
    /// the DDS graph, issue 1686). Only a blind send is ever repeated.
    blind: bool,
    /// Set once a result has arrived.
    done: bool,
    /// Issue 1918 — set once "waiting for the action server" has been logged.
    waiting_logged: bool,
}

impl ExecutableNode for FibonacciClient {
    type State = State;

    fn init() -> Self::State {
        State {
            sent_at: None,
            blind: false,
            done: false,
            waiting_logged: false,
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        match callback.as_str() {
            "on_feedback" => {
                if let Ok(f) = ctx.message::<FibonacciFeedback>() {
                    log::info!("Next number in sequence received: {:?}", f.sequence);
                }
            }
            "on_result" => {
                if let Ok(r) = ctx.message::<FibonacciResult>() {
                    state.done = true;
                    log::info!("Result received: {:?}", r.sequence);
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
        if let Some(at) = state.sent_at {
            // A send the backend could CONFIRM a server for is never repeated.
            // A blind one is: XRCE's Agent matches the client's request writer
            // with the server's reader asynchronously, a goal written before
            // that match is dropped, and with no answer coming back this client
            // would wait forever (the `rust_xrce_action` cell failed ~1 run in
            // 3 exactly so: "Goal sent", then nothing). A goal that did arrive
            // gets its result long before `BLIND_RESEND`, so a repeat is only
            // ever a goal that was lost.
            if !state.blind || now.saturating_sub(at) < BLIND_RESEND {
                return;
            }
            log::info!("No result yet; sending the goal again");
        }
        // Issue 1918 — wait for the server before sending. A goal sent before
        // the server has declared its send_goal service is answered by nobody.
        // `Err` means the backend cannot say (no discovery channel): send
        // blind, and repeat on silence (above).
        let blind = match ctx.action_server_is_ready_for_name("/fibonacci") {
            Ok(false) => {
                if !state.waiting_logged {
                    log::info!("Waiting for action server...");
                    state.waiting_logged = true;
                }
                return;
            }
            Ok(true) => false,
            Err(_) => true,
        };
        let goal = FibonacciGoal { order: 10 };
        log::info!("Sending goal");
        // 32 B is more than enough for one `i32` + CDR header.
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent_at = Some(now);
            state.blind = blind;
            // Issue 1918 — SENT, not accepted: `send_goal_for_name` does not
            // wait for the server's answer, so this cannot claim one.
            log::info!("Goal sent, waiting for result");
        }
        // On send failure `sent_at` is unchanged — the next tick retries.
    }
}

nros::node!(FibonacciClient);
