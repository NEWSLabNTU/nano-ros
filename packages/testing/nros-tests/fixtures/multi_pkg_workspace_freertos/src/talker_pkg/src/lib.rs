//! Phase 212.M.5.a.3 fixture — talker component (Rust, no_std).
//!
//! Declares a single node. The workspace's `firmware` bin registers it
//! through the run plan its `build.rs` generates (one `<pkg>::register`
//! line per launch `<node>`); the `freertos_firmware` fixture row
//! cross-builds that firmware and `freertos_firmware_entry` checks the
//! prebuilt `run_plan.rs`. The node-only declaration dates from the M.5.a.3
//! BSP baker, which dispatched it from ~~`nros_system_run`~~ through a
//! ~~`DeclarativeSlot`~~ that fired no timer / subscription callbacks.

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
