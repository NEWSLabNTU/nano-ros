//! NuttX QEMU ARM Talker Example
//!
//! Publishes `std_msgs/Int32` messages on `/chatter`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use nros::prelude::*;
use nros_nuttx_qemu_arm::{Config, run};
use std_msgs::msg::Int32;

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("talker")?;

        println!("Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        println!("Publisher declared");
        println!("Publishing messages...");

        let mut count: i32 = 0;
        loop {
            // Poll to process network events (~1s between publishes)
            for _ in 0..100 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            match publisher.publish(&Int32 { data: count }) {
                Ok(()) => println!("Published: {}", count),
                Err(e) => eprintln!("Publish failed: {:?}", e),
            }
            count = count.wrapping_add(1);
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
