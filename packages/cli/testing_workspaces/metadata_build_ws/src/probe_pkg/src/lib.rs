#![no_std]

//! Phase 172.E discovery probe — a minimal nros component. `nros metadata
//! --build` compiles this in metadata mode and runs `register` against the
//! recorder to emit `node.metadata.json`.

pub mod node {
    use nros::{
        CallbackId, ComponentContext, ComponentResult, EntityId, NodeId, NodeOptions,
        TimerDuration,
    };

    pub struct Component;

    impl nros::Component for Component {
        const NAME: &'static str = "node";

        fn register(context: &mut ComponentContext<'_>) -> ComponentResult<()> {
            let mut node =
                context.create_node(NodeId::new("probe_node"), NodeOptions::new("probe"))?;
            let _timer = node.create_timer(
                EntityId::new("tick"),
                CallbackId::new("cb_tick"),
                TimerDuration::from_millis(100),
            )?;
            Ok(())
        }
    }
}
