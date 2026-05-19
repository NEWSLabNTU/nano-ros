//! Native Listener Example
//!
//! Demonstrates subscribing to messages using nros on native x86.
//! Uses the Executor API with callback-based subscriptions.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Then run the listener:
//! cargo run -p native-rs-listener
//! ```
//!
//! # Diagnostics
//!
//! Output flows through `nros-log`. Default writer renders
//! `[<LEVEL>] listener: <message>` on stderr.

use nros::prelude::*;
use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};
use std_msgs::msg::Int32;

static LOGGER: Logger = Logger::new("listener");

extern crate nros_platform_cffi as _;

fn init_logging() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());
}

/// Safety-e2e listener: validates CRC and tracks sequence gaps.
#[cfg(feature = "safety-e2e")]
fn main() {
    init_logging();

    nros_info!(&LOGGER, "nros Native Listener (Zenoh Transport, Safety E2E)");

    let config = ExecutorConfig::from_env().node_name("listener");
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    let mut count: u64 = 0;
    executor
        .register_subscription_with_safety::<Int32, _>("/chatter", move |msg, status| {
            count += 1;
            let crc_str = match status.crc_valid {
                Some(true) => "ok",
                Some(false) => "FAIL",
                None => "n/a",
            };
            nros_info!(
                &LOGGER,
                "[{}] Received: data={} [SAFETY] seq_gap={} dup={} crc={}",
                count, msg.data, status.gap, status.duplicate, crc_str
            );
        })
        .expect("Failed to add safety subscription");
    nros_info!(&LOGGER, "Safety subscriber created for topic: /chatter");
    nros_info!(&LOGGER, "Waiting for Int32 messages on /chatter...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        nros_error!(&LOGGER, "Spin error: {:?}", e);
    }
}

/// Standard listener with MessageInfo: logs seq/gid at trace level.
#[cfg(not(feature = "safety-e2e"))]
fn main() {
    init_logging();

    nros_info!(&LOGGER, "nros Native Listener (Zenoh Transport)");

    let config = ExecutorConfig::from_env().node_name("listener");
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    executor
        .register_subscription_with_info::<Int32, _>("/chatter", move |msg, msg_info| {
            nros_info!(&LOGGER, "Received: {}", msg.data);
            if let Some(msg_info) = msg_info {
                let gid = msg_info.publisher_gid();
                nros_trace!(
                    &LOGGER,
                    "seq={} gid={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} ",
                    msg_info.publication_sequence_number(),
                    gid[0], gid[1], gid[2], gid[3],
                    gid[4], gid[5], gid[6], gid[7],
                );
            }
        })
        .expect("Failed to add subscription");
    nros_info!(&LOGGER, "Subscriber created for topic: /chatter");
    nros_info!(&LOGGER, "Waiting for Int32 messages on /chatter...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        nros_error!(&LOGGER, "Spin error: {:?}", e);
    }
}
