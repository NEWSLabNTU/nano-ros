//! FreeRTOS QEMU MPS2-AN385 Fibonacci action client —
//! Phase 212.L Node pkg.
//!
//! Phase 212.M.5.b — declarative-metadata-only.
//! Service-client runtime body deferred to M-F.4 (TickCtx call() seam) —
//! the same dependency applies to action-client send_goal /
//! feedback-stream wiring. The declarative metadata below is the stable
//! contract; the generated runtime will own the imperative driver once
//! the seam ships.

#![no_std]

use example_interfaces::action::Fibonacci;
use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};

pub struct FibonacciClient;

impl Node for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client = node.create_action_client_for_name::<Fibonacci>("/fibonacci")?;
        Ok(())
    }
}

impl ExecutableNode for FibonacciClient {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        // Phase 212.M.5.b — declarative-metadata-only.
        // Service-client runtime body deferred to M-F.4
        // (TickCtx call() seam). Codegen-system will own the imperative
        // driver once the seam ships.
    }
}

nros::node!(FibonacciClient);
