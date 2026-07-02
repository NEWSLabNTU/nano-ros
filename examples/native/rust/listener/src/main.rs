//! Native Listener Example — Phase 212.L.2 Application pkg shape.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and logs each message.
//! Single-file `[[bin]]`: explicit [`nros::init_with_launch_auto`]
//! (Pattern 2 — picks up launch overlay env vars from the environment)
//! then a user-owned spin loop.
//!
//! # Usage
//!
//! ```bash
//! # Zenoh path (default):
//! zenohd --listen tcp/127.0.0.1:7447 &
//! cargo run -p native-rs-listener
//!
//! # XRCE path:
//! cargo run -p native-rs-listener --no-default-features --features rmw-xrce
//!
//! # Cyclone DDS path (Phase 212.K pure-cargo):
//! cargo run -p native-rs-listener --no-default-features --features rmw-cyclonedds
//! ```
//!
//! Override the locator with `NROS_LOCATOR` (or legacy `ZENOH_LOCATOR`);
//! `RUST_LOG=debug` for debug logs.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();
    info!("nros Native Listener");
    info!("==========================================");

    // Phase 212.L.5 Pattern 2 — launch-aware init.
    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");
    // Default `/chatter`; override with `NROS_SUB_TOPIC` so the same binary can
    // subscribe to another Int32 topic (e.g. `/sum` for the service-showcase e2e).
    let topic: &'static str = match std::env::var("NROS_SUB_TOPIC") {
        Ok(t) if !t.is_empty() => Box::leak(t.into_boxed_str()),
        _ => "/chatter",
    };
    executor
        .node_mut(nid)
        .subscription(topic)
        .typed::<Int32>()
        .message_info()
        .build(move |msg, info| {
            info!("Received: {}", msg.data);
            if let Some(info) = info {
                let gid = info.publisher_gid();
                log::trace!(
                    "seq={} gid={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} ",
                    info.publication_sequence_number(),
                    gid[0],
                    gid[1],
                    gid[2],
                    gid[3],
                    gid[4],
                    gid[5],
                    gid[6],
                    gid[7],
                );
            }
        })
        .expect("Failed to add subscription");
    info!("Subscriber created for topic: {topic}");
    info!("Waiting for Int32 messages on {topic}...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
