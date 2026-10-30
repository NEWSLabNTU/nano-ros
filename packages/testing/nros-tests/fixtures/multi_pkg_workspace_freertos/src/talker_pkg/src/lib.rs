//! Phase 212.M.5.a.3 fixture — talker component (Rust, no_std).
//!
//! Declares a single node. The BSP baker linked this pkg's mangled
//! register symbol (`__nros_component_talker_pkg_register`, emitted by
//! `nros::node!()`) into the firmware and dispatched it from
//! ~~`nros_system_run`~~; the M.5.a.2 BSP-side ~~`DeclarativeSlot`~~ fired
//! no timer / subscription callbacks, so the node-only declaration was the
//! whole M.5.a.3 link + dispatch contract. The baker, that dispatcher and
//! the slot are gone, and nothing in the tree builds this fixture now.

#![no_std]

use nros::{Component, NodeContext, NodeOptions, NodeResult};

pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("talker"))?;
        Ok(())
    }
}

// Phase 212.M.5.a.4 — the macro now emits `_init` / `_dispatch` /
// `_tick` symbols that call into `ExecutableNode`. Declarative
// pkgs satisfy that contract with the no-op blanket impl.
nros::declarative_component!(Talker);
nros::node!(Talker);
