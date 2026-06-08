//! Phase 212.M.5.a.3 fixture — listener component (Rust, no_std).
//!
//! Companion to `talker_pkg`. See the talker for rationale.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("listener"))?;
        Ok(())
    }
}

// Phase 212.M.5.a.4 — see the talker pkg for rationale.
nros::declarative_component!(Listener);
nros::node!(Listener);
