//! Simple ESP32 WiFi Talker using nros-esp32
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with the QEMU bsp-talker to see the similar API.
//!
//! # Building
//!
//! ```bash
//! SSID=MyNetwork PASSWORD=secret cargo +nightly build --release
//! ```
//!
//! # Flashing
//!
//! ```bash
//! SSID=MyNetwork PASSWORD=secret cargo +nightly run --release
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros::prelude::*;
use nros_esp32::{NodeConfig, WifiConfig, entry, esp_println, run};
use std_msgs::msg::Int32;

/// WiFi credentials (set via environment variables at compile time)
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

nros_esp32::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run(NodeConfig::new(WifiConfig::new(SSID, PASSWORD)), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("talker")?;

        // Declare publisher
        esp_println::println!("Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        esp_println::println!("Publisher declared");

        // Publish messages
        esp_println::println!("");
        esp_println::println!("Publishing messages...");

        for i in 0..10i32 {
            // Poll to process network events
            for _ in 0..100 {
                executor.spin_once(10);
            }

            if let Err(e) = publisher.publish(&Int32 { data: i }) {
                esp_println::println!("Publish failed: {:?}", e);
            } else {
                esp_println::println!("Published: {}", i);
            }
        }

        esp_println::println!("");
        esp_println::println!("Done publishing 10 messages.");

        Ok::<(), NodeError>(())
    })
}
