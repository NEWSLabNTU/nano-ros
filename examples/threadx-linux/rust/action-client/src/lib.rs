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
