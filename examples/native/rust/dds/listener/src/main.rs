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
    // Phase 115.L.5 — install dust-dds C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_dds::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    executor
        .register_subscription::<Int32, _>("/chatter", move |msg| {
            info!("Received: {}", msg.data);
        })
        .expect("Failed to add subscription");
    info!("Subscriber created for topic: /chatter");

    info!("Waiting for Int32 messages on /chatter...");
    info!("(Press Ctrl+C to exit)");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
