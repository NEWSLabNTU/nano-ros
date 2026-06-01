//! Phase 212.M.5.a.3 fixture — listener component (Rust, no_std).
//!
//! Companion to `talker_pkg`. See the talker for rationale.

#![no_std]

use nros::{Component, ComponentContext, ComponentResult, NodeId, NodeOptions};

pub struct Listener;

impl Component for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let _node = ctx.create_node(NodeId::new("node"), NodeOptions::new("listener"))?;
        Ok(())
    }
}

nros::component!(Listener);
