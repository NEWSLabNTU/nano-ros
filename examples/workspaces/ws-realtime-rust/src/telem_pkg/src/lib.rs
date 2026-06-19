//! Phase 228.G fixture — telemetry node (tier `low`).

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult, TimerDuration};

pub struct Telem;

impl Node for Telem {
    const NAME: &'static str = "telem_node";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("telem_node"))?;
        node.callback_group("telem")?;
        let _t = node.create_timer_for_callback_name("on_telem", TimerDuration::from_millis(100))?;
        Ok(())
    }
}

nros::declarative_component!(Telem);
nros::node!(Telem);
