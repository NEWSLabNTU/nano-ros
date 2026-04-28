//! ThreadX Linux DDS Listener (Phase 97.4.threadx-linux).

use nros::prelude::*;
use nros_board_threadx_linux::{Config, run};
use std_msgs::msg::Int32;

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new("")
            .domain_id(config.domain_id)
            .node_name("dds_listener");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("dds_listener")?;

        println!("Subscribing to /chatter (std_msgs/Int32) over DDS");
        let mut subscription = node.create_subscription::<Int32>("/chatter")?;
        println!("Subscriber declared");
        println!("Waiting for messages...");

        loop {
            executor.spin_once(core::time::Duration::from_millis(10));
            if let Some(msg) = subscription.try_recv()? {
                println!("Received: {}", msg.data);
            }
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
