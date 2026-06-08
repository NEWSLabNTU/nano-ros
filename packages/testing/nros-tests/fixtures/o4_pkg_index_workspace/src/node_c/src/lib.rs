//! Phase 212.O.4 fixture — Node pkg `node_c`. See sibling `node_a`.

#![no_std]

use nros::{Node, NodeContext, NodeOptions, NodeResult};

pub struct NodeC;

impl Node for NodeC {
    const NAME: &'static str = "node_c";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(NodeOptions::new("node_c"))?;
        Ok(())
    }
}

nros::declarative_component!(NodeC);
nros::node!(NodeC);
