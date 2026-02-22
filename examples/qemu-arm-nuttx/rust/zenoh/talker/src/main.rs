//! NuttX QEMU ARM Talker Example
//!
//! Publishes `std_msgs/Int32` messages on `/chatter`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use nros::prelude::*;
use nros_nuttx_qemu_arm::{Config, run};
use std_msgs::msg::Int32;

fn main() {
    run(Config::default(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
        let mut node = executor.create_node("talker")?;

        println!("Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        println!("Publisher declared");
        println!();
        println!("Publishing messages...");

        for i in 0..10i32 {
            // Poll to process network events
            for _ in 0..100 {
                executor.spin_once(10);
            }

            match publisher.publish(&Int32 { data: i }) {
                Ok(()) => println!("Published: {}", i),
                Err(e) => eprintln!("Publish failed: {:?}", e),
            }
        }

        println!();
        println!("Done publishing 10 messages.");
        Ok::<(), NodeError>(())
    })
}
