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

use nros::prelude::*;
use nros_log::{Logger, nros_debug, nros_error, nros_info, nros_trace, nros_warn};
use std_msgs::msg::Int32;

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("listener");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros Native Listener (DDS/RTPS Transport)");
    nros_info!(&LOGGER, "============================================");

    let config = ExecutorConfig::from_env().node_name("listener");
    // Phase 115.L.5 — install dust-dds C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_dds::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    executor
        .register_subscription::<Int32, _>("/chatter", move |msg| {
            nros_info!(&LOGGER, "Received: {}", msg.data);
        })
        .expect("Failed to add subscription");
    nros_info!(&LOGGER, "Subscriber created for topic: /chatter");

    nros_info!(&LOGGER, "Waiting for Int32 messages on /chatter...");
    nros_info!(&LOGGER, "(Press Ctrl+C to exit)");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        nros_error!(&LOGGER, "Spin error: {:?}", e);
    }
}
