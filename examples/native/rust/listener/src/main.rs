//! Native Listener Example — Phase 212.L.2 Application pkg shape.
//!
//! Subscribes to `std_msgs/String` on `/chatter` and logs each message
//! (`I heard: [Hello World: N]`), matching the official ROS 2
//! `demo_nodes_cpp` listener.
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
use std_msgs::msg::{Int32, String as StringMsg};

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
    let topic = "/chatter";
    // The message type is baked into the wire keyexpr, so the subscriber's
    // type must match the publisher's. Default is `std_msgs/String` (the
    // canonical talker demo); `NROS_SUB_TYPE=int32` observes the workspace
    // Int32 demo (`talker_pkg`) instead — used by the ws-entry E2E, whose
    // Entry publishes Int32 on `/chatter`.
    let sub_type = std::env::var("NROS_SUB_TYPE").unwrap_or_default();
    if sub_type.eq_ignore_ascii_case("int32") {
        executor
            .node_mut(nid)
            .subscription(topic)
            .typed::<Int32>()
            .build(move |msg| {
                info!("I heard: [{}]", msg.data);
            })
            .expect("Failed to add subscription");
    } else {
        executor
            .node_mut(nid)
            .subscription(topic)
            .typed::<StringMsg>()
            .message_info()
            .build(move |msg, info| {
                info!("I heard: [{}]", msg.data);
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
    }
    info!("Subscriber created for topic: {topic}");
    info!("Waiting for messages on {topic}...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
