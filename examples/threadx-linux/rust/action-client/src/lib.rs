//! ThreadX Linux Action Client — Phase 212.L Component pkg.
//!
//! Declares an `example_interfaces/Fibonacci` action client on
//! `/fibonacci`. The Component pkg currently only declares the
//! client surface — goal dispatch + result polling land with the
//! W.5.6 client-side tick API. The generated runtime owns init /
//! executor / spin.

#![no_std]

use example_interfaces::action::Fibonacci;
use nros::{
    Component, ComponentContext, ComponentResult, EntityId, NodeId, NodeOptions,
    declarative_component,
};

pub struct ActionClient;

impl Component for ActionClient {
    const NAME: &'static str = "action_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(
            NodeId::new("node"),
            NodeOptions::new("fibonacci_action_client"),
        )?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("cli_fib"), "/fibonacci")?;
        Ok(())
    }
}

declarative_component!(ActionClient);

nros::component!(ActionClient);

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
