//! NuttX QEMU ARM AddTwoInts service client — Phase 212.L Component pkg.
//!
//! Declarative metadata: node + service client + driver timer. The
//! component model expresses *what* entities exist; the imperative call
//! sequencing (issue request → await reply) currently has no `TickCtx`
//! seam — service-client invocation is a follow-up wave for the
//! generated runtime. For now the body is a declarative no-op; codegen-
//! system will own the call-loop once that seam lands.

#![no_std]

use example_interfaces::srv::AddTwoInts;
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions, TimerDuration,
};

pub struct AddTwoIntsClient;

impl Component for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
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

impl ExecutableComponent for AddTwoIntsClient {
    /// Index into the canned test-case table for the next call.
    type State = u8;

    fn init() -> Self::State {
        0
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // The runtime-side service-client call seam is a follow-up wave
        // (TickCtx today exposes publish + action ops only). Codegen-
        // system will wire the imperative call loop here once the seam
        // ships; the declarative metadata above is the stable contract.
    }
}

nros::component!(AddTwoIntsClient);

/// Phase 212.N.7 step-2 — codegen-facing `register` entry point.
///
/// See the `talker` Component pkg sibling for full docs. Generic over
/// `R: ?Sized` so the Component pkg's Cargo.toml does not need a
/// direct `nros-platform` dep; the Entry pkg monomorphises `R` to
/// `nros_platform::RuntimeCtx<'_>`. Body is a no-op until the 212.N
/// runtime plumbing lands.
pub fn register<R: ?Sized>(_runtime: &mut R) -> Result<(), &'static str> {
    Ok(())
}
