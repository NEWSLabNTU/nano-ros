//! Native DDS Talker Example
//!
//! Phase 122.4 — L2 timer-driven publisher. Demonstrates publishing
//! messages using nros with the DDS/RTPS backend. Brokerless
//! peer-to-peer discovery — no router or agent needed.
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

    let publisher = {
        let mut node = executor
            .create_node("talker")
            .expect("Failed to create node");
        info!("Node created: talker");
        let pub_ = node
            .create_publisher::<Int32>("/chatter")
            .expect("Failed to create publisher");
        info!("Publisher created for topic: /chatter");
        pub_
    };

    let mut count: i32 = 0;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            let msg = Int32 { data: count };
            match publisher.publish(&msg) {
                Ok(()) => info!("Published: {}", count),
                Err(e) => error!("Publish error: {:?}", e),
            }
            count = count.wrapping_add(1);
        })
        .expect("Failed to register publish timer");
    info!("Publishing Int32 messages every 1s...");

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}
