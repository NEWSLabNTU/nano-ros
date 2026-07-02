//! Nested-message talker (fixture for the non-flat bridge e2e).
//!
//! Moved out of `examples/native/rust/talker` (it was the `header` cfg-gated
//! code) so the example stays cfg-free. Publishes `std_msgs/Int32` on
//! `/chatter` AND a NESTED `std_msgs/Header` (a `builtin_interfaces/Time`
//! stamp and a string `frame_id`) on `/header` every 1 s, from two nodes in
//! one session (the executor dedups sessions), so the declarative bridge's
//! typed `register::<Header>` egress can be exercised with live data.
//!
//! Consumed by `tests/declarative_bridge_zenoh_to_cyclonedds.rs`.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();
    // Zenoh-only fixture: register the backend explicitly (the examples route
    // this through `nros_board_native::register_linked_rmw()`).
    nros_rmw_zenoh::register().expect("register zenoh backend");

    info!("nros Native Talker (nested /header variant)");

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

    // A NESTED publisher on /header so the declarative bridge's typed
    // `register::<Header>` egress sees live data. Own node (the executor
    // dedups sessions; 2 nodes < MAX_NODES).
    let header_pub = {
        let mut node = executor
            .create_node("talker_header")
            .expect("Failed to create header node");
        node.create_publisher::<std_msgs::msg::Header>("/header")
            .expect("Failed to create /header publisher")
    };

    let mut count: i32 = 0;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            let msg = Int32 { data: count };
            match publisher.publish(&msg) {
                Ok(()) => info!("Published: {}", count),
                Err(e) => error!("Publish error: {:?}", e),
            }
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
            count = count.wrapping_add(1);
        })
        .expect("Failed to register publish timer");
    info!("Publishing Int32 + Header messages every 1s...");

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}
