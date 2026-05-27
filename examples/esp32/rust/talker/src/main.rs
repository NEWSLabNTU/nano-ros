//! Simple ESP32 WiFi Talker using nros-board-esp32
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with the QEMU bsp-talker to see the similar API.
//!
//! Network configuration is in `config.toml` (WiFi credentials,
//! zenoh locator, optional static IP).
//!
//! Phase 88.16.E — user diagnostics route through `nros-log`. The
//! board crate's `run()` registers an `esp_println`-backed writer
//! against `nros-platform-esp32`'s fn-ptr slot, so the example only
//! needs to install sinks.
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
use nros_board_esp32::{NodeConfig, entry, run};
use nros_log::{Logger, nros_error, nros_info};
use std_msgs::msg::Int32;

nros_board_esp32::esp_bootloader_esp_idf::esp_app_desc!();

static LOGGER: Logger = Logger::new("talker");

#[entry]
fn main() -> ! {
    run(
        NodeConfig::from_toml(include_str!("../nros.toml")),
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("talker");
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let publisher = {
                let mut node = executor.create_node("talker")?;
                nros_info!(&LOGGER, "Declaring publisher on /chatter (std_msgs/Int32)");
                node.create_publisher::<Int32>("/chatter")?
            };
            nros_info!(&LOGGER, "Publisher declared");
            nros_info!(&LOGGER, "Publishing messages...");

            let mut count: i32 = 0;
            executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
                match publisher.publish(&Int32 { data: count }) {
                    Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                    Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
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
