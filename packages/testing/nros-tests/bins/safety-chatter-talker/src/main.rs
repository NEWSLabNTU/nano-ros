//! E2E-safety talker (cross-process publisher fixture).
//!
//! The talker half of the safety e2e pair: publishes `std_msgs/Int32` on
//! `/chatter` every 1 s with the zenoh backend's `safety-e2e` CRC attach
//! baked in (a manifest feature of the backend — there is no safety-specific
//! application code on the publish side). Extracted from the
//! `examples/native/rust/talker --features safety-e2e` fixture build so the
//! example manifest stays free of test-only features.
//!
//! Paired with `tests/safety_e2e.rs`: the safety / declarative safety
//! listeners validate the CRC + sequence this publisher attaches.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();
    // Zenoh-only fixture: register the backend explicitly (the examples route
    // this through `nros_board_native::register_linked_rmw()`).
    nros_rmw_zenoh::register().expect("register zenoh backend");

    info!("nros Native Talker (Safety E2E)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("talker");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

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
