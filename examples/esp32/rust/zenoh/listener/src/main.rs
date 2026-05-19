//! Simple ESP32 WiFi Listener using nros-board-esp32
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with the QEMU bsp-listener to see the similar API.
//!
//! Network configuration is in `config.toml` (WiFi credentials,
//! zenoh locator, optional static IP).
//!
//! Phase 88.16.E — user diagnostics route through `nros-log`. The
//! board crate's `run()` registers an `esp_println`-backed writer
//! against `nros-platform-esp32`'s fn-ptr slot.

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros::prelude::*;
use nros_board_esp32::{NodeConfig, entry, run};
use nros_log::{nros_info, Logger};
use std_msgs::msg::Int32;

nros_board_esp32::esp_bootloader_esp_idf::esp_app_desc!();

static LOGGER: Logger = Logger::new("listener");

#[entry]
fn main() -> ! {
    run(
        NodeConfig::from_toml(include_str!("../config.toml")),
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("listener");
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("listener")?;

            nros_info!(&LOGGER, "Subscribing to /chatter (std_msgs/Int32)");
            executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                nros_info!(&LOGGER, "Received: {}", msg.data);
            })?;

            nros_info!(&LOGGER, "Subscriber declared");
            nros_info!(&LOGGER, "Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
