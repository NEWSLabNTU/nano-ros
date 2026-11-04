//! AddServer Node pkg — the ROS 2 tutorial `add_two_ints_server`: serves
//! `example_interfaces/srv/AddTwoInts` on `/add_two_ints` and logs each request.
//!
//! phase-535 (RFC-0122 D1, option B): the node logic is the standalone example's
//! (`examples/<platform>/rust/service-server/src/lib.rs`), so the example pairs and the
//! workspace images run one program. The workspace keeps its own class and node
//! names, which its bringup, launch files and tests address.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult,
};

pub struct AddServer;

impl Component for AddServer {
    const NAME: &'static str = "add_server";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 1, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_server"))?;
        let _srv = node.create_service_server_for_name_with_callback::<AddTwoInts>(
            "/add_two_ints",
            "handle_add",
        )?;
        log::info!("Waiting for service requests");
        Ok(())
    }
}

impl ExecutableNode for AddServer {
    type State = ();
    // phase-521 W6 — no `tick`, so the entry may park past 10 ms.
    const HAS_TICK: bool = false;

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "handle_add" {
            if let Ok(req) = ctx.message::<AddTwoIntsRequest>() {
                log::info!("Incoming request");
                log::info!("a: {} b: {}", req.a, req.b);
                let reply = AddTwoIntsResponse { sum: req.a + req.b };
                let _ = ctx.reply::<AddTwoIntsResponse, 64>(&reply);
            }
        }
    }
}

nros::node!(AddServer);
