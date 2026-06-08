//! Phase 212.O.4 fixture — Node pkg `node_b`. See sibling `node_a`.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct NodeB;

impl Node for NodeB {
    const NAME: &'static str = "node_b";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("node_b"))?;
        Ok(())
    }
}

nros::declarative_component!(NodeB);
nros::node!(NodeB);
