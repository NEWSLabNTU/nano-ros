//! QEMU MPS2-AN385 RTIC AddTwoInts Service Client — phase-244.D1 node logic.
//!
//! Calls an `example_interfaces/AddTwoInts` service on `/add_two_ints`.
//! Declarative, platform/RMW-agnostic Node: `register()` declares node +
//! service client + a 1 Hz driver timer; `on_callback("issue_call")` arms the
//! next request (bumps the operand counter); `tick()` dispatches the call —
//! `TickCtx` is the only place `&mut Executor` is free. The entry crate's
//! `nros::main!()` + the RTIC board (`nros-board-rtic-mps2-an385`) own
//! hardware/network bring-up, executor open, RMW registration, and the RTIC
//! dispatch loop. Locator/domain live in the entry's
//! `[package.metadata.nros.deploy.rtic-mps2-an385]` — never here.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
    TimerDuration,
};

/// AddTwoInts service client — issues a call once per second.
pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_client"))?;
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
        // Stack-buf sizes: AddTwoInts request = 2 × i64 + CDR header = 24 B;
        // response = 1 × i64 + header = 16 B. 64 each is generous.
        let _: nros::NodeResult<AddTwoIntsResponse> = ctx
            .call_for_name::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>("/add_two_ints", &req);
    }
}

nros::node!(AddTwoIntsClient);
