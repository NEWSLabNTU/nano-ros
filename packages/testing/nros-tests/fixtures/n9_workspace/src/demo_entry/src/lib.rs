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

use nros::{Component, ComponentContext, ComponentResult, NodeId, NodeOptions};

pub struct DemoEntry;

impl Component for DemoEntry {
    const NAME: &'static str = "demo_entry";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let _node =
            ctx.create_node(NodeId::new("node"), NodeOptions::new("demo_entry"))?;
        Ok(())
    }
}

nros::declarative_component!(DemoEntry);
nros::node!(DemoEntry);
