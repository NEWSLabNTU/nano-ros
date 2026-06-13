//! Zephyr AddTwoInts service client — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Declarative metadata: node + service client + driver timer.
//!
//! Phase 212.M-F.4.b transcription: timer fires → `on_callback` flips
//! the state's `pending` flag + bumps the operand counter. Real call
//! dispatch lives in `tick` (the only place `&mut Executor` is free —
//! see `TickCtx` docs). Until `nros::TickCtx::call`'s underlying
//! `ClientDispatch` impl ships in the installed nros-cli (M-F.4.a),
//! the in-tree `UnsupportedClients` stub returns `NodeDeclError::
//! Runtime`; the body still compiles + the seam is honest.

#![no_std]

extern crate zephyr;

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
    TimerDuration,
};

pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        let _client = node.create_service_client_for_name::<AddTwoInts>("/add_two_ints")?;
        let _timer =
            node.create_timer_for_callback_name("issue_call", TimerDuration::from_secs(1))?;
        Ok(())
    }
}

pub struct State {
    /// Set by `on_callback` when the timer fires; drained by `tick`
    /// after dispatching the call.
    pending: bool,
    /// Monotonic counter used as the request operands.
    counter: i64,
}

impl ExecutableNode for AddTwoIntsClient {
    type State = State;

    fn init() -> Self::State {
        State {
            pending: false,
            counter: 0,
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "issue_call" {
            state.pending = true;
            state.counter = state.counter.wrapping_add(1);
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if !state.pending {
            return;
        }
        state.pending = false;
        let req = AddTwoIntsRequest {
            a: state.counter,
            b: state.counter.wrapping_add(1),
        };
        // Canonical service-client fixture marker — the e2e harness asserts on
        // "Response: sum=<n>". The single-node runtime now dispatches the call
        // for real (phase-212 M-F.23); log the reply so the test can observe it.
        match ctx
            .call_for_name::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>("/add_two_ints", &req)
        {
            Ok(resp) => log::info!("Response: sum={}", resp.sum),
            Err(_) => log::info!("service call failed"),
        }
    }
}

nros::node!(AddTwoIntsClient);
nros::zephyr_component_main!(AddTwoIntsClient);
