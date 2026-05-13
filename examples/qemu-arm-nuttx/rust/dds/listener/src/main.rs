//! NuttX QEMU ARM DDS Listener (Phase 97.4.nuttx).
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` over the brokerless
//! DDS / RTPS backend (`rmw-dds`).

use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
use std_msgs::msg::Int32;

fn main() {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new("")
                .domain_id(config.domain_id)
                .node_name("dds_listener");
            // Phase 115.L.5 — install dust-dds C-vtable backend.
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("dds_listener")?;

            println!("Subscribing to /chatter (std_msgs/Int32) over DDS");
            executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                println!("Received: {}", msg.data);
            })?;
            println!("Subscriber declared");
            println!("Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
