//! Generic Int32 source (fixture for cross-process e2e observation).
//!
//! The publishing twin of `bins/int32-sink`. Publishes `std_msgs/Int32` on
//! every topic named in `NROS_PUB_TOPICS` (comma-separated, default
//! `/chatter`) every `NROS_PUB_PERIOD_MS` milliseconds (default 100), the same
//! counter value on each, and prints `Published: N` per tick
//! ([`nros_tests::output::INT32_TALKER_LOG_PREFIX`]).
//!
//! phase-482 W3 — written as the peer of the `topic-state-monitor-port` cells,
//! whose ported node watches TWO topics and reports each one's liveness: one
//! process feeds both, and the period is short enough that a monitor's 500 ms
//! "live" window always holds a fresh sample. The fixture talkers in this
//! directory publish on a fixed `/chatter` once a second, so none could.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();
    nros_board_linux::register_linked_rmw();

    let topics: Vec<&'static str> = std::env::var("NROS_PUB_TOPICS")
        .ok()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "/chatter".to_string())
        .split(',')
        .map(|t| &*Box::leak(t.trim().to_string().into_boxed_str()))
        .filter(|t| !t.is_empty())
        .collect();
    let period_ms: u64 = std::env::var("NROS_PUB_PERIOD_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    info!("nros Int32 Source Talker (test fixture)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("int32_source");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let publishers = {
        let mut node = executor
            .create_node("int32_source")
            .expect("Failed to create node");
        let mut pubs = Vec::with_capacity(topics.len());
        for topic in &topics {
            pubs.push(
                node.create_publisher::<Int32>(topic)
                    .expect("Failed to create publisher"),
            );
            info!("Publisher created for topic: {topic}");
        }
        pubs
    };

    let mut count: i32 = 0;
    executor
        .register_timer(nros::TimerDuration::from_millis(period_ms), move || {
            let msg = Int32 { data: count };
            for publisher in &publishers {
                if let Err(e) = publisher.publish(&msg) {
                    error!("Publish error: {:?}", e);
                }
            }
            info!("Published: {}", count);
            count = count.wrapping_add(1);
        })
        .expect("Failed to register publish timer");
    info!("Publishing Int32 on {topics:?} every {period_ms} ms");

    if let Err(e) = executor.spin(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
