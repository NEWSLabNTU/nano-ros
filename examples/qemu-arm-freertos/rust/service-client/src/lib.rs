//! FreeRTOS QEMU MPS2-AN385 AddTwoInts service client —
//! Phase 212.L Component pkg.
//!
//! Phase 212.M.5.b — declarative-metadata-only.
//! Service-client runtime body deferred to M-F.4 (TickCtx call() seam).
//!
//! The component model expresses *what* entities exist; the imperative
//! call sequencing (issue request → await reply) currently has no
//! `TickCtx` seam — service-client invocation is a follow-up wave for
//! the generated runtime. The body is a declarative no-op stub.

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
        let mut node = ctx.create_node(
            NodeId::new("node"),
            NodeOptions::new("add_two_ints_client"),
        )?;
        let _client = node.create_service_client::<AddTwoInts>(
            EntityId::new("client_add"),
            "/add_two_ints",
        )?;
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
        // Phase 212.M.5.b — declarative-metadata-only.
        // Service-client runtime body deferred to M-F.4
        // (TickCtx call() seam). Codegen-system will wire the imperative
        // call loop here once the seam ships; the declarative metadata
        // above is the stable contract.
    }
}

nros::component!(AddTwoIntsClient);
