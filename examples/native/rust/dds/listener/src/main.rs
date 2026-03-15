//! Native DDS Listener Example
//!
//! Demonstrates subscribing to messages using nros with the DDS/RTPS backend.
//! Uses brokerless peer-to-peer discovery — no router or agent needed.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p native-dds-listener
//! ```

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    info!("nros Native Listener (DDS/RTPS Transport)");
    info!("============================================");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    let mut count: u64 = 0;
    executor
        .add_subscription::<Int32, _>("/chatter", move |msg| {
            count += 1;
            info!("[{}] Received: data={}", count, msg.data);
        })
        .expect("Failed to add subscription");
    info!("Subscriber created for topic: /chatter");

    info!("Waiting for Int32 messages on /chatter...");
    info!("(Press Ctrl+C to exit)");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
