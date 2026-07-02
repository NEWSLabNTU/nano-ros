//! Native Talker Example — Phase 212.L.2 Application pkg shape.
//!
//! Publishes `std_msgs/Int32` on `/chatter` every 1 s using nros on
//! native x86. Single-file `[[bin]]`: explicit
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

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

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

    #[cfg(feature = "param-services")]
    {
        executor
            .register_parameter_services()
            .expect("Failed to register parameter services");
        executor.declare_parameter("start_value", ParameterValue::Integer(0));
        info!("Parameter services registered for /talker");
    }

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

    // phase-267 (non-flat bridge e2e) — a NESTED publisher on /header so the
    // declarative bridge's typed-`register::<Header>` egress can be exercised with
    // live data. Own node (the executor dedups sessions; 2 nodes < MAX_NODES).
    #[cfg(feature = "header")]
    let header_pub = {
        let mut node = executor
            .create_node("talker_header")
            .expect("Failed to create header node");
        node.create_publisher::<std_msgs::msg::Header>("/header")
            .expect("Failed to create /header publisher")
    };

    #[cfg(feature = "param-services")]
    let counter_start = {
        let v = executor.get_parameter_integer("start_value").unwrap_or(0) as i32;
        info!("Counter start value: {}", v);
        v
    };
    #[cfg(not(feature = "param-services"))]
    let counter_start = 0i32;

    let mut count: i32 = counter_start;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            let msg = Int32 { data: count };
            match publisher.publish(&msg) {
                Ok(()) => info!("Published: {}", count),
                Err(e) => error!("Publish error: {:?}", e),
            }
            #[cfg(feature = "header")]
            {
                let hdr = std_msgs::msg::Header {
                    stamp: builtin_interfaces::msg::Time {
                        sec: count,
                        nanosec: 0,
                    },
                    frame_id: Default::default(),
                };
                match header_pub.publish(&hdr) {
                    Ok(()) => info!("Published Header: {}", count),
                    Err(e) => error!("Header publish error: {:?}", e),
                }
            }
            count = count.wrapping_add(1);
        })
        .expect("Failed to register publish timer");
    info!("Publishing Int32 messages every 1s...");

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}
