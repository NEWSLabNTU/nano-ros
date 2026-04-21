//! Simple ESP32 WiFi Talker using nros-esp32
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with the QEMU bsp-talker to see the similar API.
//!
//! Network configuration is in `config.toml` (WiFi credentials,
//! zenoh locator, optional static IP).
//!
//! # Building
//!
//! ```bash
//! cargo +nightly build --release
//! ```
//!
//! # Flashing
//!
//! ```bash
//! cargo +nightly run --release
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros::prelude::*;
use nros_esp32::{NodeConfig, entry, esp_println, run};
use std_msgs::msg::Int32;

nros_esp32::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run(
        NodeConfig::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("talker");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("talker")?;

            esp_println::println!("Declaring publisher on /chatter (std_msgs/Int32)");
            let publisher = node.create_publisher::<Int32>("/chatter")?;
            esp_println::println!("Publisher declared");

            esp_println::println!("Publishing messages...");

            let mut count: i32 = 0;
            loop {
                for _ in 0..100 {
                    executor.spin_once(core::time::Duration::from_millis(10));
                }

                match publisher.publish(&Int32 { data: count }) {
                    Ok(()) => esp_println::println!("Published: {}", count),
                    Err(e) => esp_println::println!("Publish failed: {:?}", e),
                }
                count = count.wrapping_add(1);
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
