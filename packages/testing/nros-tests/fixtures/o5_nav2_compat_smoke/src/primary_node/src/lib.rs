//! Phase 212.O.5 fixture — primary Component pkg (Rust, no_std).
//!
//! Implements `nros::Node` for the `Primary` struct and stamps it with
//! `nros::node!()`. The latter emits the `pub fn register(runtime)`
//! symbol the codegen-emitted `run_plan(runtime)` body calls.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct Primary;

impl Node for Primary {
    const NAME: &'static str = "primary";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("primary"))?;
        Ok(())
    }
}

nros::declarative_component!(Primary);
nros::node!(Primary);
