//! Native Talker Example — Phase 212.L.2 Application pkg shape.
//!
//! Publishes `std_msgs/String` (`Hello World: N`) on `/chatter` every
//! 1 s using nros on native x86, matching the official ROS 2
//! `demo_nodes_cpp` talker. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] (Pattern 2 — picks up launch
//! overlay env vars from the environment) then a user-owned spin
//! loop.
//!
//! # Usage
//!
//! ```bash
//! # Zenoh path (default):
//! zenohd --listen tcp/127.0.0.1:7447 &
//! cargo run -p native-rs-talker
//!
//! # XRCE path:
//! cargo run -p native-rs-talker --no-default-features --features rmw-xrce
//!
//! # Cyclone DDS path (Phase 212.K pure-cargo):
//! cargo run -p native-rs-talker --no-default-features --features rmw-cyclonedds
//! ```
//!
//! Override the locator at runtime with `NROS_LOCATOR` (or the legacy
//! `ZENOH_LOCATOR`). Enable debug logs with `RUST_LOG=debug`.

use core::fmt::Write as _;

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::{Int32, String as StringMsg};

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();
    info!("nros Native Talker");
    info!("=========================================");

    // Phase 212.L.5 Pattern 2 — launch-aware init. Picks up
    // `ROS_DOMAIN_ID` / `NROS_LOCATOR` / `NROS_SESSION_MODE` /
    // `RMW_IMPLEMENTATION` from the environment, otherwise falls
    // back to the standard env-var defaults.
    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("talker");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    // The message type is baked into the wire keyexpr; publish the type the
    // subscriber expects. Default is `std_msgs/String` (the canonical talker
    // demo); `NROS_PUB_TYPE=int32` publishes `std_msgs/Int32` instead — used by
    // the cross-RMW ws-bridge e2e, whose demo forwards Int32 on /chatter.
    let pub_type = std::env::var("NROS_PUB_TYPE").unwrap_or_default();
    if pub_type.eq_ignore_ascii_case("int32") {
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
                count = count.wrapping_add(1);
                let msg = Int32 { data: count };
                match publisher.publish(&msg) {
                    Ok(()) => info!("Publishing: {}", msg.data),
                    Err(e) => error!("Publish error: {:?}", e),
                }
            })
            .expect("Failed to register publish timer");
    } else {
        let publisher = {
            let mut node = executor
                .create_node("talker")
                .expect("Failed to create node");
            info!("Node created: talker");
            let pub_ = node
                .create_publisher::<StringMsg>("/chatter")
                .expect("Failed to create publisher");
            info!("Publisher created for topic: /chatter");
            pub_
        };
        let mut count: i32 = 0;
        executor
            .register_timer(nros::TimerDuration::from_millis(1000), move || {
                count = count.wrapping_add(1);
                let mut msg = StringMsg::default();
                let _ = write!(msg.data, "Hello World: {count}");
                match publisher.publish(&msg) {
                    Ok(()) => info!("Publishing: '{}'", msg.data),
                    Err(e) => error!("Publish error: {:?}", e),
                }
            })
            .expect("Failed to register publish timer");
    }

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}
