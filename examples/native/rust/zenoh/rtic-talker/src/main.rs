//! Native RTIC-pattern Talker
//!
//! Validates the RTIC integration pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `publisher.publish()` (independent of executor)
//!
//! This is the native equivalent of `examples/stm32f4/rust/zenoh/rtic-talker/`.

use log::info;
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    info!("nros RTIC-pattern Talker (native)");

    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("talker")
        .expect("Failed to create node");
    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");

    info!("Publishing Int32 on /chatter (RTIC pattern)...");

    // Stabilization delay (like RTIC Mono::delay(2000.millis()))
    for _ in 0..200 {
        executor.spin_once(0);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    for i in 0..10i32 {
        match publisher.publish(&Int32 { data: i }) {
            Ok(()) => info!("[{}] Published: data={}", i, i),
            Err(e) => log::error!("Publish error: {:?}", e),
        }

        // Drive I/O with spin_once(0) — non-blocking, like RTIC net_poll task
        for _ in 0..100 {
            executor.spin_once(0);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    info!("Done publishing 10 messages");
}
