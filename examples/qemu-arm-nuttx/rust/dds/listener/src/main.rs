//! NuttX QEMU ARM DDS Listener (Phase 97.4.nuttx).
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` over the brokerless
//! DDS / RTPS backend (`rmw-dds`).

use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
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
