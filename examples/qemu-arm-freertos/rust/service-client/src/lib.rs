//! FreeRTOS QEMU MPS2-AN385 AddTwoInts service client —
//! Phase 212.L Node pkg.
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
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};

pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        let _client = node.create_service_client_for_name::<AddTwoInts>("/add_two_ints")?;
        let _timer =
            node.create_timer_for_callback_name("issue_call", TimerDuration::from_secs(1))?;
        Ok(())
    }
}

impl ExecutableNode for AddTwoIntsClient {
    /// Index into the canned test-case table for the next call.
    type State = u8;

    fn init() -> Self::State {
        0
    }

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        // Phase 212.M.5.b — declarative-metadata-only.
        // Service-client runtime body deferred to M-F.4
        // (TickCtx call() seam). Codegen-system will wire the imperative
        // call loop here once the seam ships; the declarative metadata
        // above is the stable contract.
    }
}

nros::node!(AddTwoIntsClient);
