//! Phase 212.N.9 fixture — minimal Node pkg.
//!
//! Implements `nros::Node` for the `Talker` struct and stamps
//! it with `nros::node!()`. The latter emits the
//! `pub fn register(runtime)` symbol the `nros::main!()` macro's
//! emitted body calls.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("talker"))?;
        Ok(())
    }
}

nros::declarative_component!(Talker);
nros::node!(Talker);
