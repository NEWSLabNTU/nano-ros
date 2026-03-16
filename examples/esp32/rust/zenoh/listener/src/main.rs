//! Simple ESP32 WiFi Listener using nros-esp32
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with the QEMU bsp-listener to see the similar API.
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
                .node_name("listener");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("listener")?;

            esp_println::println!("Subscribing to /chatter (std_msgs/Int32)");
            let mut subscription = node.create_subscription::<Int32>("/chatter")?;

            esp_println::println!("Subscriber declared");
            esp_println::println!("");
            esp_println::println!("Waiting for messages...");

            let mut msg_count = 0u32;
            let mut poll_count = 0u32;

            loop {
                executor.spin_once(10);

                if let Some(msg) = subscription.try_recv()? {
                    msg_count += 1;
                    esp_println::println!("Received [{}]: {}", msg_count, msg.data);

                    if msg_count >= 10 {
                        esp_println::println!("");
                        esp_println::println!("Received 10 messages.");
                        break;
                    }
                }

                poll_count += 1;
                if poll_count > 1_000_000 {
                    esp_println::println!("");
                    esp_println::println!("Timeout waiting for messages.");
                    break;
                }
            }

            Ok::<(), NodeError>(())
        },
    )
}
