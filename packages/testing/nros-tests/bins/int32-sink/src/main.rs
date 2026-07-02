//! Generic Int32 sink (fixture for cross-process e2e observation).
//!
//! Moved out of `examples/native/rust/listener` in phase-277 W4: the example
//! flipped to the official ROS 2 demo behavior (`std_msgs/String`,
//! `I heard: [Hello World: N]`) and dropped its test-only `NROS_SUB_TOPIC`
//! escape hatch. This bin keeps the old sink behavior for the tests that
//! observe an Int32 side topic (`/sum`, `/fib_result`, `/safe_ok`, `/ctrl`,
//! `/telem`, or an Int32 `/chatter` from the fixture talkers): subscribe to
//! `NROS_SUB_TOPIC` (default `/chatter`) typed `std_msgs/Int32` and print
//! `Received: N` per message
//! ([`nros_tests::output::INT32_LISTENER_LOG_PREFIX`]).

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();
    // Zenoh-only fixture: register the backend explicitly (the examples route
    // this through `nros_board_native::register_linked_rmw()`).
    nros_rmw_zenoh::register().expect("register zenoh backend");

    // Banner deliberately contains "Listener": the e2e spawn helpers key
    // readiness off that word (pre-W4 they waited on the example listener's
    // "nros Native Listener" banner).
    info!("nros Int32 Sink Listener (test fixture)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");
    // Default `/chatter`; override with `NROS_SUB_TOPIC` so the same binary can
    // observe any Int32 topic (e.g. `/sum` for the service-showcase e2e).
    let topic: &'static str = match std::env::var("NROS_SUB_TOPIC") {
        Ok(t) if !t.is_empty() => Box::leak(t.into_boxed_str()),
        _ => "/chatter",
    };
    executor
        .node_mut(nid)
        .subscription(topic)
        .typed::<Int32>()
        .build(move |msg| {
            info!("Received: {}", msg.data);
        })
        .expect("Failed to add subscription");
    info!("Subscriber created for topic: {topic}");
    info!("Waiting for Int32 messages on {topic}...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
