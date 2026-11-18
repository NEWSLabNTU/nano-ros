//! The urgent level of the RTIC levels cell: a 100 Hz timer whose callback
//! logs the clock the moment it runs.
//!
//! The cell reads these times, not the executor's own lateness report: the
//! executor credits a band's timers per pass, so its report is only as honest
//! as the wake it is meant to test. `URGENT_FIRE <now_us> <late_us>` gives both.

#![no_std]

use nros::{
    Callback, CallbackCtx, Component, DeclarativeNode, ExecutableNode, NodeContext, NodeOptions,
    NodeResult, TimerDuration,
};

/// The period the contract's 100 Hz derives the urgent level from.
pub const PERIOD_US: u64 = 10_000;

/// A 100 Hz timer, nothing else.
pub struct Urgent;

impl Component for Urgent {
    const NAME: &'static str = "urgent";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("urgent"))?;
        // One group, so the derived schedule has a group to place on a level.
        node.callback_group("main")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_micros(PERIOD_US))?;
        Ok(())
    }
}

impl ExecutableNode for Urgent {
    type State = ();
    const HAS_TICK: bool = false;

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let now = nros::time::now_us();
            let late_us = ctx
                .timer_info()
                .map_or(-1, |i| (i.actual_call_time - i.expected_call_time) / 1_000);
            log::info!("URGENT_FIRE {} {}", now, late_us);
        }
    }
}

nros::node!(Urgent);
