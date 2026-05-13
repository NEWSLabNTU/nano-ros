//! NuttX QEMU ARM DDS Talker (Phase 97.4.nuttx).
//!
//! Publishes `std_msgs/Int32` on `/chatter` over the brokerless DDS /
//! RTPS backend (`rmw-dds`). Sibling listener instance discovers via
//! SPDP multicast on `239.255.0.1:7400`.

use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
use std_msgs::msg::Int32;

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new("")
            .domain_id(config.domain_id)
            .node_name("dds_talker");
        // Phase 115.L.5 — install dust-dds C-vtable backend.
        let mut executor = Executor::open(&exec_config)?;
        let publisher = {
            let mut node = executor.create_node("dds_talker")?;
            println!("Declaring publisher on /chatter (std_msgs/Int32) over DDS");
            node.create_publisher::<Int32>("/chatter")?
        };
        println!("Publisher declared");
        println!("Publishing messages...");

        let mut count: i32 = 0;
        executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
            match publisher.publish(&Int32 { data: count }) {
                Ok(()) => println!("Published: {}", count),
                Err(e) => println!("Publish failed: {:?}", e),
            }
            count = count.wrapping_add(1);
        })?;

        loop {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
