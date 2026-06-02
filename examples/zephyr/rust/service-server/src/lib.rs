//! Zephyr AddTwoInts service server — Phase 212.M.3 / Phase 212.L Component pkg.
//!
//! Declarative: node + service server with a `handle_add` callback.
//! Body: reads typed request, writes typed reply through the W.5.3 reply
//! sink. Generated runtime owns init / executor / spin.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions,
};

pub struct AddTwoIntsServer;

impl Component for AddTwoIntsServer {
    const NAME: &'static str = "add_two_ints_server";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node =
            ctx.create_node(NodeId::new("node"), NodeOptions::new("add_two_ints_server"))?;
        let _srv = node.create_service_server::<AddTwoInts>(
            EntityId::new("srv_add"),
            CallbackId::new("handle_add"),
            "/add_two_ints",
        )?;
        Ok(())
    }
}

impl ExecutableComponent for AddTwoIntsServer {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "handle_add" {
            if let Ok(req) = ctx.message::<AddTwoIntsRequest>() {
                let reply = AddTwoIntsResponse { sum: req.a + req.b };
                let _ = ctx.reply::<AddTwoIntsResponse, 64>(&reply);
            }
        }
    }
}

nros::component!(AddTwoIntsServer);

/// Phase 212.N.7 step-2 — codegen-facing `register` entry point.
///
/// Zephyr is the §212.N.2 carve-out: `nros-board-zephyr` is
/// `NetworkWait`-only, and Kconfig + DTS own the C `main()` boot
/// path (a Rust staticlib can't take over `main` on Zephyr). There
/// is therefore **no Entry pkg sibling** for Zephyr Component pkgs;
/// the existing `zephyr.exe`-from-`west build` shape stays.
///
/// This wrapper exists so a future Zephyr-side codegen layer can
/// call `<this-pkg>::register(runtime)?` from inside the C
/// `main()`'s `nros_app_rust_entry` hook — the same stable surface
/// signature as the other §212.N.7 Component pkgs, just driven from
/// C rather than a Rust Entry pkg.
///
/// The 212.N runtime plumbing that lets this function reach into
/// the executor + register the [`AddTwoIntsServer`] component lands
/// in a follow-up step. For now the body is intentionally a no-op
/// (the existing `nros::component!(AddTwoIntsServer)` macro still
/// owns the symbol-export path the M.5.b Zephyr baker consumes).
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
