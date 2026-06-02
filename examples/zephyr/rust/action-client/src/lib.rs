//! Zephyr Fibonacci action client — Phase 212.M.3 / Phase 212.L Component pkg.
//!
//! Declarative: node + action client.
//!
//! Phase 212.M-F.4.b transcription: one-shot `send_goal` on the first
//! `tick` call (after registration completes). Feedback + result
//! callbacks land via `on_callback` once codegen wires the feedback-
//! stream + result-subscriber + `GoalStatusArray` topics through to
//! dispatch. The in-tree `UnsupportedClients` stub returns
//! `ComponentError::Runtime` from `send_goal_raw` until the M-F.4.a
//! `GenClientDispatch` reaches the installed nros-cli.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions, TickCtx,
};

pub struct FibonacciClient;

impl Component for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(
            NodeId::new("node"),
            NodeOptions::new("fibonacci_action_client"),
        )?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("client_fib"), "/fibonacci")?;
        Ok(())
    }
}

pub struct State {
    /// Set once the goal has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableComponent for FibonacciClient {
    type State = State;

    fn init() -> Self::State {
        State { sent: false }
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // Feedback / result callbacks land here once codegen wires the
        // `GoalStatusArray` + feedback-stream + result-future
        // subscribers. The id-driven dispatch is the M-F.4.a + N
        // runtime plumbing; this body is the seam.
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        if ctx
            .send_goal::<FibonacciGoal, 32>(EntityId::new("client_fib"), &goal)
            .is_ok()
        {
            state.sent = true;
        }
    }
}

nros::component!(FibonacciClient);

/// Phase 212.N.7 step-2 — codegen-facing `register` entry point.
///
/// Zephyr is the §212.N.2 carve-out: `nros-board-zephyr` is
/// `NetworkWait`-only, and Kconfig + DTS own the C `main()` boot
/// path (a Rust staticlib can't take over `main` on Zephyr). There
/// is therefore **no Entry pkg sibling** for Zephyr Component pkgs;
/// the existing `zephyr.exe`-from-`west build` shape stays.
///
/// This wrapper exists so a future Zephyr-side codegen layer can
/// call `<this-pkg>::register(runtime)?` from inside the C
/// `main()`'s `nros_app_rust_entry` hook — the same stable surface
/// signature as the other §212.N.7 Component pkgs, just driven from
/// C rather than a Rust Entry pkg.
///
/// The 212.N runtime plumbing that lets this function reach into
/// the executor + register the [`FibonacciClient`] component lands
/// in a follow-up step. For now the body is intentionally a no-op
/// (the existing `nros::component!(FibonacciClient)` macro still
/// owns the symbol-export path the M.5.b Zephyr baker consumes).
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
