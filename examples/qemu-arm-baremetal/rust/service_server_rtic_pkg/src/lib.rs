//! QEMU MPS2-AN385 RTIC AddTwoInts Service Server — phase-244.D1 node logic.
//!
//! Serves an `example_interfaces/AddTwoInts` service on `/add_two_ints`.
//! Declarative, platform/RMW-agnostic Node: `register()` declares node + service
//! server; `on_callback("on_add")` reads the typed request, sums the two ints,
//! and writes the typed reply. The entry crate's `nros::main!()` + the RTIC board
//! (`nros-board-rtic-mps2-an385`) own hardware/network bring-up, executor open,
//! RMW registration, and the RTIC dispatch loop. Locator/domain live in the
//! entry's `[package.metadata.nros.deploy.rtic-mps2-an385]` — never here.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};

/// AddTwoInts service server — sums the two request ints on every call.
pub struct AddTwoIntsServer;

impl Node for AddTwoIntsServer {
    const NAME: &'static str = "add_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_server"))?;
        let _srv = node.create_service_server_for_name_with_callback::<AddTwoInts>(
            "/add_two_ints",
            "on_add",
        )?;
        Ok(())
    }
}

impl ExecutableNode for AddTwoIntsServer {
    /// Count of handled requests.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_add" {
            if let Ok(req) = ctx.message::<AddTwoIntsRequest>() {
                let resp = AddTwoIntsResponse { sum: req.a + req.b };
                let _ = ctx.reply::<AddTwoIntsResponse, 64>(&resp);
                *state = state.wrapping_add(1);
            }
        }
    }
}

nros::node!(AddTwoIntsServer);
