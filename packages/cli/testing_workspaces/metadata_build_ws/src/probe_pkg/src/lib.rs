#![no_std]

//! Phase 172.E discovery probe — a minimal nros component. `nros metadata
//! --build` compiles this in metadata mode and runs `register` against the
//! recorder to emit `node.metadata.json`.

pub mod node {
    use nros::{Node, NodeContext, NodeOptions, NodeResult, TimerDuration};

    pub struct Component;

    impl Node for Component {
        const NAME: &'static str = "node";

        fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
            let mut node = context.create_node(NodeOptions::new("probe"))?;
            let _timer =
                node.create_timer_for_callback_name("cb_tick", TimerDuration::from_millis(100))?;
            Ok(())
        }
    }
}
