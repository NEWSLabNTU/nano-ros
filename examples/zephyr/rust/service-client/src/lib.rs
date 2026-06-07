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
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeId, NodeOptions,
    NodeResult, TickCtx, TimerDuration,
};

pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node =
            ctx.create_node(NodeId::new("node"), NodeOptions::new("add_two_ints_client"))?;
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
        let _: nros::NodeResult<AddTwoIntsResponse> = ctx
            .call::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>(
                EntityId::new("client_add"),
                &req,
            );
    }
}

nros::node!(AddTwoIntsClient);
nros::zephyr_component_main!(AddTwoIntsClient);
