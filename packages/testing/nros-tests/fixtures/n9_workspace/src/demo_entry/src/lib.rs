//! Phase 212.N.9 fixture — `demo_entry` lib component.
//!
//! Form-1 (`nros::main!()` with no args) emits
//! `::demo_entry::register(runtime)?;` — the macro derives the
//! crate ident from `CARGO_PKG_NAME` and expects the lib target to
//! expose a `pub fn register(runtime)`.
//!
//! The cargo bin-target automatically gets `extern crate demo_entry;`
//! so the bin's `nros::main!()` resolves this `register` symbol.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct DemoEntry;

impl Node for DemoEntry {
    const NAME: &'static str = "demo_entry";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("demo_entry"))?;
        Ok(())
    }
}

nros::declarative_component!(DemoEntry);
nros::node!(DemoEntry);
