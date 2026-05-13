//! Simple ESP32-C3 QEMU Talker using nros-board-esp32-qemu
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
use nros_board_esp32_qemu::{esp_println, prelude::*};
use std_msgs::msg::Int32;

nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("talker");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let publisher = {
                let mut node = executor.create_node("talker")?;
                esp_println::println!("Declaring publisher on /chatter (std_msgs/Int32)");
                node.create_publisher::<Int32>("/chatter")?
            };
            esp_println::println!("Publisher declared");

            esp_println::println!("Publishing messages...");

            let mut count: i32 = 0;
            executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
                match publisher.publish(&Int32 { data: count }) {
                    Ok(()) => esp_println::println!("Published: {}", count),
                    Err(e) => esp_println::println!("Publish failed: {:?}", e),
                }
                count = count.wrapping_add(1);
            })?;

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
