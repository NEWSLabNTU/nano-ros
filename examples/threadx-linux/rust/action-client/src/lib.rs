//! ThreadX Linux Action Client — Phase 212.L Component pkg.
//!
//! Declares an `example_interfaces/Fibonacci` action client on
//! `/fibonacci`. Phase 212.M-F.4.b transcription: one-shot
//! `send_goal` on the first `tick` call. Feedback + result
//! callbacks land via `on_callback` once codegen wires the feedback-
//! stream + result-subscriber + `GoalStatusArray` topics through
//! to dispatch. The in-tree `UnsupportedClients` stub returns
//! `ComponentError::Runtime` until the M-F.4.a `GenClientDispatch`
//! reaches the installed nros-cli.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions, TickCtx,
};

pub struct ActionClient;

impl Component for ActionClient {
    const NAME: &'static str = "action_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(
            NodeId::new("node"),
            NodeOptions::new("fibonacci_action_client"),
        )?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("cli_fib"), "/fibonacci")?;
        Ok(())
    }
}

pub struct State {
    /// Set once the goal has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableComponent for ActionClient {
    type State = State;

    fn init() -> Self::State {
        State { sent: false }
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 10 };
        if ctx
            .send_goal::<FibonacciGoal, 32>(EntityId::new("cli_fib"), &goal)
            .is_ok()
        {
            state.sent = true;
        }
    }
}

nros::component!(ActionClient);

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
