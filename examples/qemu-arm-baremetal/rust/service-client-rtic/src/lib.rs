//! QEMU MPS2-AN385 RTIC AddTwoInts Service Client node logic.
//!
//! Calls an `example_interfaces/AddTwoInts` service on `/add_two_ints`.
//! Declarative, platform/RMW-agnostic Node: `register()` declares node +
//! service client + a 1 Hz driver timer; `on_callback("issue_call")` arms the
//! (single) request; `tick()` dispatches the call — `TickCtx` is the only
//! place `&mut Executor` is free. The timer keeps firing after the reply so
//! the task structure stays alive, but only one request is ever issued (the
//! timer retries until the first call succeeds — that doubles as discovery
//! warm-up). The entry crate's `nros::main!()` + the RTIC board
//! (`nros-board-rtic-mps2-an385`) own hardware/network bring-up, executor
//! open, RMW registration, and the RTIC dispatch loop. Locator/domain live in
//! the entry's `[package.metadata.nros.deploy.rtic-mps2-an385]` — never here.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
    TimerDuration,
};
use nros_log::{Logger, nros_info};

// Diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("add_two_ints_client");

/// AddTwoInts service client — issues one fixed `(2, 3)` request.
pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        nros_log::register_logger(&LOGGER);
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
    /// Set once a reply has been received — no further requests are issued.
    done: bool,
}

impl ExecutableNode for AddTwoIntsClient {
    type State = State;

    fn init() -> Self::State {
        State {
            pending: false,
            done: false,
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "issue_call" && !state.done {
            state.pending = true;
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if !state.pending || state.done {
            return;
        }
        state.pending = false;
        // One fixed request — embedded client, no argv.
        let req = AddTwoIntsRequest { a: 2, b: 3 };
        // Stack-buf sizes: AddTwoInts request = 2 × i64 + CDR header = 24 B;
        // response = 1 × i64 + header = 16 B. 64 each is generous.
        // On error (e.g. the server is not discovered yet) the next timer
        // fire retries — this doubles as the discovery warm-up.
        if let Ok(resp) = ctx
            .call_for_name::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>("/add_two_ints", &req)
        {
            nros_info!(&LOGGER, "Result of add_two_ints: {}", resp.sum);
            state.done = true;
        }
    }
}

nros::node!(AddTwoIntsClient);
