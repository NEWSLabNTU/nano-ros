//! ThreadX Linux Talker
//!
//! Publishes `std_msgs/Int32` messages on `/chatter`.

use nros::prelude::*;
use nros_threadx_linux::{Config, run};
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
            for _ in 0..100 {
                executor.spin_once(10);
            }

            if let Err(e) = publisher.publish(&Int32 { data: i }) {
                println!("Publish failed: {:?}", e);
            } else {
                println!("Published: {}", i);
            }
        }

        println!();
        println!("Done publishing 10 messages.");

        Ok::<(), NodeError>(())
    })
}
