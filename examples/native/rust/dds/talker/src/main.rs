//! Native DDS Talker Example
//!
//! Demonstrates publishing messages using nros with the DDS/RTPS backend.
//! Uses brokerless peer-to-peer discovery — no router or agent needed.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p native-dds-talker
//! ```

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    info!("nros Native Talker (DDS/RTPS Transport)");
    info!("==========================================");

    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    let mut node = executor
        .create_node("talker")
        .expect("Failed to create node");
    info!("Node created: talker");

    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");
    info!("Publisher created for topic: /chatter");
    info!("Publishing Int32 messages...");

    let mut count: i32 = 0;
    loop {
        let msg = Int32 { data: count };
        match publisher.publish(&msg) {
            Ok(()) => info!("Published: {}", count),
            Err(e) => error!("Publish error: {:?}", e),
        }
        count = count.wrapping_add(1);

        executor.spin_once(core::time::Duration::from_millis(10));
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
