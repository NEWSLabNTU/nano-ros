//! Native RTIC-pattern Listener
//!
//! Validates the RTIC integration pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `subscription.try_recv()` (manual polling)
//!
//! This is the native equivalent of `examples/stm32f4/rust/zenoh/rtic-listener/`.

use log::info;
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    info!("nros RTIC-pattern Listener (native)");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor = Executor::<_, 0, 0>::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("listener")
        .expect("Failed to create node");
    let mut subscription = node
        .create_subscription::<Int32>("/chatter")
        .expect("Failed to create subscription");

    info!("Waiting for Int32 messages on /chatter (RTIC pattern)...");

    let mut count = 0u32;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

    while std::time::Instant::now() < deadline {
        executor.spin_once(0);

        match subscription.try_recv() {
            Ok(Some(msg)) => {
                count += 1;
                info!("[{}] Received: data={}", count, msg.data);
            }
            Ok(None) => {}
            Err(e) => log::error!("Receive error: {:?}", e),
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    info!("Done. Received {} messages total", count);
}
