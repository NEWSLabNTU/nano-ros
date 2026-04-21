//! nros STM32F4 Talker Example using BSP
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`,
//! compatible with ROS 2 nodes via rmw_zenoh.
//!
//! # Hardware
//!
//! - Board: NUCLEO-F429ZI (or similar STM32F4 with Ethernet)
//! - Connect Ethernet cable to the board's RJ45 port
//!
//! # Network Configuration
//!
//! Default (static IP):
//! - Device IP: 192.168.1.10/24
//! - Gateway: 192.168.1.1
//! - Zenoh Router: 192.168.1.1:7447
//!
//! # Building
//!
//! ```bash
//! cargo build --release
//! cargo run --release  # Uses probe-rs to flash
//! ```

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

// defmt 0.3 requires a timestamp function in each binary crate
defmt::timestamp!("{=u64:us}", { 0 });

use nros::prelude::*;
use nros_stm32f4::nros_platform_stm32f4::clock::clock_ms;
use nros_stm32f4::prelude::*;
use std_msgs::msg::Int32;

/// Poll interval in milliseconds
const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(10);

/// Publish interval in milliseconds
const PUBLISH_INTERVAL_MS: u32 = 1000;

#[entry]
fn main() -> ! {
    run(Config::nucleo_f429zi(), |config| -> Result<(), NodeError> {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("talker")?;

        info!("Creating publisher for /chatter (std_msgs/Int32)...");
        let publisher = node.create_publisher::<Int32>("/chatter")?;

        info!("Starting publish loop (1 Hz)...");
        let mut counter: i32 = 0;
        let mut last_publish_ms: u64 = 0;

        loop {
            executor.spin_once(POLL_INTERVAL);

            let now_ms = clock_ms();
            if now_ms - last_publish_ms >= PUBLISH_INTERVAL_MS as u64 {
                last_publish_ms = now_ms;
                counter = counter.wrapping_add(1);

                match publisher.publish(&Int32 { data: counter }) {
                    Ok(()) => info!("Published: {}", counter),
                    Err(e) => warn!("Publish failed: {:?}", e),
                }
            }
        }
    })
}
