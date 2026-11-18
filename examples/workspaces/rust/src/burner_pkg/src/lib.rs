//! The busy boot level of the RTIC levels cell: a 5 Hz timer whose callback
//! spins for 50 ms, logging the clock as it starts and ends.
//!
//! An urgent level that preempts it runs INSIDE `[BURN_START, BURN_END]`; one
//! that cannot runs after it.

#![no_std]

use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TimerDuration,
};

/// How long each callback spins.
pub const BURN_US: u64 = 50_000;

/// A 5 Hz timer that burns CPU.
pub struct Burner;

impl Component for Burner {
    const NAME: &'static str = "burner";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("burner"))?;
        // One group, so the derived schedule has a group to place on a level.
        node.callback_group("main")?;
        let _timer =
            node.create_timer_for_callback_name("on_burn", TimerDuration::from_millis(200))?;
        Ok(())
    }
}

impl ExecutableNode for Burner {
    type State = ();
    const HAS_TICK: bool = false;

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_burn" {
            let start = nros::time::now_us();
            log::info!("BURN_START {}", start);
            while nros::time::now_us() < start + BURN_US {
                core::hint::spin_loop();
            }
            log::info!("BURN_END {}", nros::time::now_us());
        }
    }
}

nros::node!(Burner);
