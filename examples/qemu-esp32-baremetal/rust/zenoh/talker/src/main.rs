//! Simple ESP32-C3 QEMU Talker using nros-esp32-qemu
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with qemu-bsp-talker -- this is the ESP32-C3 equivalent.
//!
//! # Building
//!
//! ```bash
//! just build-examples-esp32-qemu
//! ```
//!
//! # Running (requires QEMU with Espressif fork)
//!
//! ```bash
//! ./scripts/esp32/launch-esp32c3.sh --tap tap-qemu0 \
//!     --binary build/esp32-qemu/esp32-qemu-talker.bin
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros::prelude::*;
use nros_esp32_qemu::esp_println;
use nros_esp32_qemu::prelude::*;
use std_msgs::msg::Int32;

nros_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
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
                // Poll to process network events (~1s between publishes)
                for _ in 0..100 {
                    executor.spin_once(10);
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
