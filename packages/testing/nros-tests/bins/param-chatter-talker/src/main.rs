//! Parameterised talker (fixture for the parameter-services tests).
//!
//! Moved out of `examples/native/rust/talker` (it was the `param-services`
//! cfg-gated code) so the example stays cfg-free. Registers the REP-2002
//! parameter services, declares an integer `start_value` parameter, reads it
//! back to seed the counter, then publishes `std_msgs/Int32` on `/chatter`
//! every 1 s.
//!
//! Consumed by `tests/params.rs`: the default-value tests assert the
//! `Counter start value: 0` line; the interop tests drive `ros2 param
//! list/get/set` against the registered services over zenohd.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();
    // Zenoh-only fixture: register the backend explicitly (the examples route
    // this through `nros_board_native::register_linked_rmw()`).
    nros_rmw_zenoh::register().expect("register zenoh backend");

    info!("nros Native Talker (parameter services)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("talker");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    executor
        .register_parameter_services()
        .expect("Failed to register parameter services");
    executor.declare_parameter("start_value", ParameterValue::Integer(0));
    info!("Parameter services registered for /talker");

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

    let counter_start = {
        let v = executor.get_parameter_integer("start_value").unwrap_or(0) as i32;
        info!("Counter start value: {}", v);
        v
    };

    let mut count: i32 = counter_start;
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
