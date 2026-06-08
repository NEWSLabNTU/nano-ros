//! Phase 212.O.3 fixture — board-agnostic shared Component.
//!
//! The whole point: this source compiles UNMODIFIED under both
//! POSIX (`posix_entry`, host target) and FreeRTOS (`freertos_entry`,
//! `thumbv7m-none-eabi`). Linking the same rlib against two distinct
//! Board impls proves the Phase 212.N.4 codegen emit
//! (`OUT_DIR/run_plan.rs`) is board-agnostic.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct SharedNode;

impl Node for SharedNode {
    const NAME: &'static str = "shared_node";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("shared_node"))?;
        Ok(())
    }
}

// Phase 212.M.5.a.4 — declarative_component + node macros emit the
// register / lifecycle symbols the codegen `run_plan(runtime)` body
// invokes. Same macro expansion under both platform features —
// neither macro reads platform cfg.
nros::declarative_component!(SharedNode);
nros::node!(SharedNode);
