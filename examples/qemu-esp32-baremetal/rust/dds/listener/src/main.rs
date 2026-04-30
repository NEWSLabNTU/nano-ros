//! ESP32-C3 QEMU DDS Listener (Phase 97.3.esp32-qemu).
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` over the brokerless
//! DDS / RTPS backend (`rmw-dds`).

#![no_std]
#![no_main]

extern crate alloc;

use esp_backtrace as _;
use nros::prelude::*;
use nros_board_esp32_qemu::esp_println;
use nros_board_esp32_qemu::prelude::*;
use std_msgs::msg::Int32;

nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new("")
                .domain_id(config.domain_id)
                .node_name("dds_listener");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("dds_listener")?;

            esp_println::println!("Subscribing to /chatter (std_msgs/Int32) over DDS");
            let mut subscription = node.create_subscription::<Int32>("/chatter")?;
            esp_println::println!("Subscriber declared");
            esp_println::println!("Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
                if let Some(msg) = subscription.try_recv()? {
                    esp_println::println!("Received: {}", msg.data);
                }
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
