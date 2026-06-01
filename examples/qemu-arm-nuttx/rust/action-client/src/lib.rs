//! NuttX QEMU ARM Fibonacci action client — Phase 212.L Component pkg.
//!
//! Declarative: node + action client. Like the service-client variant
//! the imperative goal-sending sequence has no `TickCtx` seam yet —
//! action-client send_goal + feedback-stream wiring is a follow-up
//! wave for the generated runtime. The declarative metadata above is
//! the stable contract.

#![no_std]

use example_interfaces::action::Fibonacci;
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions,
};

pub struct FibonacciClient;

impl Component for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(
            NodeId::new("node"),
            NodeOptions::new("fibonacci_action_client"),
        )?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("client_fib"), "/fibonacci")?;
        Ok(())
    }
}

impl ExecutableComponent for FibonacciClient {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // Action-client send_goal / feedback-stream wiring is a follow-up
        // runtime wave; codegen-system will own the imperative driver
        // once the seam ships.
    }
}

nros::component!(FibonacciClient);
