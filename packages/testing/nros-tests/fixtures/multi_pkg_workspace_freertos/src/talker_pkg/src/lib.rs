//! Phase 212.M.5.a.3 fixture — talker component (Rust, no_std).
//!
//! Declares a single node. The BSP baker links this pkg's mangled
//! register symbol (`__nros_component_talker_pkg_register`, emitted by
//! `nros::component!()`) into the firmware and dispatches it from
//! `nros_system_run`. The M.5.a.2 BSP-side `DeclarativeSlot` does
//! not fire timer / subscription callbacks today (M.5.a.4 follow-up)
//! so a typed publisher / timer here would add link-time weight
//! without execution coverage; the node-only declaration exercises
//! the M.5.a.3 link + dispatch contract end-to-end.

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

// Phase 212.M.5.a.4 — the macro now emits `_init` / `_dispatch` /
// `_tick` symbols that call into `ExecutableComponent`. Declarative
// pkgs satisfy that contract with the no-op blanket impl.
nros::declarative_component!(Talker);
nros::component!(Talker);
