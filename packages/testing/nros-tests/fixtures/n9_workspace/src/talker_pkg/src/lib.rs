//! Phase 212.N.9 fixture — minimal Node pkg.
//!
//! Implements `nros::Component` for the `Talker` struct and stamps
//! it with `nros::node!()`. The latter emits the
//! `pub fn register(runtime)` symbol the `nros::main!()` macro's
//! emitted body calls.

#![no_std]

use nros::{Component, ComponentContext, ComponentResult, NodeId, NodeOptions};

pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let _node = ctx.create_node(NodeId::new("node"), NodeOptions::new("talker"))?;
        Ok(())
    }
}

nros::declarative_component!(Talker);
nros::node!(Talker);
