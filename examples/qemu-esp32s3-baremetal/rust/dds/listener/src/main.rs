//! ESP32-S3 QEMU DDS Listener (Phase 117).
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` over the brokerless
//! DDS / RTPS backend (`rmw-dds`).

#![no_std]
#![no_main]

extern crate alloc;

use esp_backtrace as _;
use nros::prelude::*;
use nros_board_esp32s3_qemu::{esp_println, prelude::*};
use std_msgs::msg::Int32;

nros_board_esp32s3_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new("")
                .domain_id(config.domain_id)
                .node_name("dds_listener");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_dds::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("dds_listener")?;

            esp_println::println!("Subscribing to /chatter (std_msgs/Int32) over DDS");
            executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                esp_println::println!("Received: {}", msg.data);
            })?;
            esp_println::println!("Subscriber declared");
            esp_println::println!("Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
