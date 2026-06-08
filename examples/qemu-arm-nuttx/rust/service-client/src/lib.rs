//! NuttX QEMU ARM AddTwoInts service client — Phase 212.L Node pkg.
//!
//! Declarative metadata: node + service client + driver timer.
//!
//! Phase 212.M-F.4.b transcription: timer fires → `on_callback` flips
//! the state's `pending` flag + bumps the operand counter. Real call
//! dispatch lives in `tick` (the only place `&mut Executor` is free —
//! see `TickCtx` docs). Once `nros::TickCtx::call` returns the typed
//! `AddTwoIntsResponse`, the body would log / store `sum`; here we
//! just observe the dispatch outcome (the in-tree `UnsupportedClients`
//! stub returns `NodeDeclError::Runtime` until the M-F.4.a-shipped
//! `GenClientDispatch` reaches the installed nros-cli).

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TickCtx, TimerDuration,
};

pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        let _client =
            node.create_service_client::<AddTwoInts>(EntityId::new("client_add"), "/add_two_ints")?;
        let _timer = node.create_timer(
            EntityId::new("timer_call"),
            CallbackId::new("issue_call"),
            TimerDuration::from_secs(1),
        )?;
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

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, _ctx: &mut CallbackCtx<'_>) {
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
            .call::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>(
                EntityId::new("client_add"),
                &req,
            );
        // Result discarded: until M-F.4.a reaches the installed CLI, the
        // runtime returns `NodeDeclError::Runtime`; once it ships, the
        // returned `AddTwoIntsResponse.sum` is what we'd log here.
    }
}

nros::node!(AddTwoIntsClient);
