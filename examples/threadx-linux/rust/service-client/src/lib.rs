//! ThreadX Linux Service Client — Phase 212.L Component pkg.
//!
//! Declares a service client for `example_interfaces/AddTwoInts` on
//! `/add_two_ints`. The generated runtime owns init / executor / spin
//! and is responsible for dispatching outbound calls (`tick`-driven in
//! the executor seam). This Component pkg currently only declares the
//! client surface — body-side call dispatch lands with W.5.6 plumbing.

#![no_std]

use example_interfaces::srv::AddTwoInts;
use nros::{
    Component, ComponentContext, ComponentResult, EntityId, NodeId, NodeOptions,
    declarative_component,
};

pub struct ServiceClient;

impl Component for ServiceClient {
    const NAME: &'static str = "service_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node =
            ctx.create_node(NodeId::new("node"), NodeOptions::new("add_two_ints_client"))?;
        let _client =
            node.create_service_client::<AddTwoInts>(EntityId::new("cli_add"), "/add_two_ints")?;
        Ok(())
    }
}

declarative_component!(ServiceClient);

nros::component!(ServiceClient);

/// Phase 212.N.7 step-2 — Entry-pkg-callable wrapper.
///
/// The codegen-emitted `run_plan(runtime)` body (see
/// `nros-build::generate_run_plan`, §212.N.4) dispatches one
/// `<pkg>::register(runtime)?` call per launch-XML `<node>` entry.
/// This wrapper is the stable per-Component-pkg API the Entry pkg
/// links against — board-agnostic, no `nros::init` / executor /
/// spin (those live in `BoardEntry::run`).
///
/// Today the wrapper is a stub: the per-component declarative
/// registration still flows through the `nros::component!`-emitted
/// trampoline that `Executor::add_components` invokes after
/// `BoardEntry::run` opens the executor. Once §212.N.4 codegen
/// lands the full `RuntimeCtx`-aware launch overlay, this body
/// will bridge `runtime` into the component's `ComponentContext`.
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
