//! ThreadX Linux Service Client — Phase 212.L Component pkg.
//!
//! Declares a service client for `example_interfaces/AddTwoInts` on
//! `/add_two_ints`. Phase 212.M-F.4.b transcription: one-shot
//! `send_request` on the first `tick` call. Until the M-F.4.a
//! `GenClientDispatch` reaches the installed nros-cli, the in-tree
//! `UnsupportedClients` stub returns `ComponentError::Runtime`; the
//! body still compiles + the seam is honest.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions, TickCtx,
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

pub struct State {
    /// Set once the call has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableComponent for ServiceClient {
    type State = State;

    fn init() -> Self::State {
        State { sent: false }
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let req = AddTwoIntsRequest { a: 7, b: 35 };
        let result: nros::ComponentResult<AddTwoIntsResponse> =
            ctx.call::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>(
                EntityId::new("cli_add"),
                &req,
            );
        if result.is_ok() {
            state.sent = true;
        }
    }
}

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
