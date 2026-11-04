//! AddClient Node pkg — the ROS 2 tutorial `add_two_ints_client`: calls
//! `/add_two_ints` ONCE with 2 + 3 and logs `Result of add_two_ints: 5`.
//!
//! A tick cannot block the executor that drives it, so the call is issued from
//! `tick` after the timer arms it, and a failed attempt is retried on the next
//! timer tick.
//!
//! phase-535 (RFC-0122 D1, option B): the node logic is the standalone example's
//! (`examples/<platform>/rust/service-client/src/lib.rs`), so the example pairs and the
//! workspace images run one program. The workspace keeps its own class and node
//! names, which its bringup, launch files and tests address.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TickCtx, TimerDuration,
};

pub struct AddClient;

impl Component for AddClient {
    const NAME: &'static str = "add_client";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 1, 0, 0);

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
    /// Set once the reply has been logged — the single-shot client
    /// then idles while the runtime keeps spinning.
    done: bool,
}

impl ExecutableNode for AddClient {
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
        // Stack-buf sizes: AddTwoInts request = 2 × i64 + CDR header = 24 B;
        // response = 1 × i64 + header = 16 B. 64 each is generous.
        match ctx
            .call_for_name::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>("/add_two_ints", &req)
        {
            Ok(reply) => {
                log::info!("Result of add_two_ints: {}", reply.sum);
                state.done = true;
            }
            Err(e) => {
                // phase-338 W3 — report the failure instead of swallowing it.
                // The silent arm made a client with no server look identical to
                // a healthy one on every platform, and `services.rs`'s
                // client-without-server test asserts exactly this marker. The
                // timer paces the retries, so this logs once per attempt.
                log::info!("Service call failed, retrying: {:?}", e);
            }
        }
    }
}

nros::node!(AddClient);
