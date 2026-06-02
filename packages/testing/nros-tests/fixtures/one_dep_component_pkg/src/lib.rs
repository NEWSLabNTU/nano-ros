//! Phase 212.M-F.13 path (b) fixture — minimal Component pkg.
//!
//! Declares one node with `nros::Component` and stamps it with the
//! `nros::component!()` macro. The macro expansion references
//! `RuntimeCtx` / `RuntimeError` / the four `Component*Fn` aliases —
//! all of which must resolve through the
//! `nros::__macro_support::nros_platform` re-export, NOT through a
//! direct `nros_platform` crate dep (this pkg's `Cargo.toml` only
//! lists `nros` under `[dependencies]`).
//!
//! `declarative_component!` supplies the blanket
//! `ExecutableComponent` impl the `register(runtime)` wrapper needs
//! when no real callback / timer body is in play. Together the two
//! macros exercise the same emit surface that production component
//! pkgs use, so a regression in the re-export path here flags before
//! it hits the freertos / threadx / nuttx Component fixtures.

#![no_std]

use nros::{Component, ComponentContext, ComponentResult, NodeId, NodeOptions};

pub struct OneDep;

impl Component for OneDep {
    const NAME: &'static str = "one_dep";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let _node = ctx.create_node(NodeId::new("node"), NodeOptions::new("one_dep"))?;
        Ok(())
    }
}

nros::declarative_component!(OneDep);
nros::component!(OneDep);
