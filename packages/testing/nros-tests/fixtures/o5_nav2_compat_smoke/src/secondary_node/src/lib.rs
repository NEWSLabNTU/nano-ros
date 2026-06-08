//! Phase 212.O.5 fixture — secondary Component pkg (Rust, no_std).
//!
//! `nros::node!()` emits the `pub fn register(runtime)` symbol the
//! codegen-emitted `run_plan(runtime)` body invokes when the primary
//! launch's `<include>` resolves this pkg's sibling launch.xml.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct Secondary;

impl Node for Secondary {
    const NAME: &'static str = "secondary";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("secondary"))?;
        Ok(())
    }
}

nros::declarative_component!(Secondary);
nros::node!(Secondary);
