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

use nros::prelude::*;
use nros_log::{Logger, nros_debug, nros_error, nros_info, nros_trace, nros_warn};
use std_msgs::msg::Int32;

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("talker");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros Native Talker (DDS/RTPS Transport)");
    nros_info!(&LOGGER, "==========================================");

    let config = ExecutorConfig::from_env().node_name("talker");
    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_dds::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    let publisher = {
        let mut node = executor
            .create_node("talker")
            .expect("Failed to create node");
        nros_info!(&LOGGER, "Node created: talker");
        let pub_ = node
            .create_publisher::<Int32>("/chatter")
            .expect("Failed to create publisher");
        nros_info!(&LOGGER, "Publisher created for topic: /chatter");
        pub_
    };

    let mut count: i32 = 0;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            let msg = Int32 { data: count };
            match publisher.publish(&msg) {
                Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                Err(e) => nros_error!(&LOGGER, "Publish error: {:?}", e),
            }
            count = count.wrapping_add(1);
        })
        .expect("Failed to register publish timer");
    nros_info!(&LOGGER, "Publishing Int32 messages every 1s...");

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}
