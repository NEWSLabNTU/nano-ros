//! Phase 228.E.2 fixture — control node (tier `high`).

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult, TimerDuration};

pub struct Control;

impl Node for Control {
    const NAME: &'static str = "control_node";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("control_node"))?;
        node.callback_group("ctrl")?;
        let _t = node.create_timer_for_callback_name("on_ctrl", TimerDuration::from_millis(10))?;
        Ok(())
    }
}

nros::declarative_component!(Control);
nros::node!(Control);
