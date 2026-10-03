//! Zephyr AddTwoInts service client.
//!
//! Declarative metadata: node + service client + driver timer.
//!
//! The timer fires → `on_callback` flips the state's `pending` flag. Real
//! call dispatch lives in `tick` (the only place `&mut Executor` is free —
//! see `TickCtx` docs). Sends ONE fixed request (2, 3); the timer retries
//! until the call succeeds (discovery warm-up), then goes quiet.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
    TimerDuration,
};

pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 1, 0, 0);

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
    /// Set once a reply has been received — the client sends ONE request.
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
        // rclcpp: `while (!client->wait_for_service(1s)) { RCLCPP_INFO(...,
        // "service not available, waiting again..."); }`. A tick cannot block
        // the executor that drives it, so the wait is one check per timer
        // tick. `Err` is "the backend cannot say" — call anyway, the request
        // is then the probe (as it always was on such backends).
        if let Ok(false) = ctx.service_is_ready_for_name("/add_two_ints") {
            log::info!("service not available, waiting again...");
            return;
        }
        let req = AddTwoIntsRequest { a: 2, b: 3 };
        match ctx
            .call_for_name::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>("/add_two_ints", &req)
        {
            Ok(resp) => {
                log::info!("Result of add_two_ints: {}", resp.sum);
                state.done = true;
            }
            // The timer re-arms `pending`, so a failed call (server not yet
            // discovered) is retried on the next fire.
            Err(_) => log::error!("service call failed, retrying"),
        }
    }
}

nros::node!(AddTwoIntsClient);
