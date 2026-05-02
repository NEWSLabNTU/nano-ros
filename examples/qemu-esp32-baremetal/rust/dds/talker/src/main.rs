//! ESP32-C3 QEMU DDS Talker (Phase 97.3.esp32-qemu).
//!
//! Publishes `std_msgs/Int32` on `/chatter` over the brokerless
//! DDS / RTPS backend (`rmw-dds`). Sibling listener instance on a
//! shared QEMU socket-mcast tunnel discovers via SPDP on
//! `239.255.0.1:7400`.

#![no_std]
#![no_main]

extern crate alloc;

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
            let exec_config = ExecutorConfig::new("")
                .domain_id(config.domain_id)
                .node_name("dds_talker");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("dds_talker")?;

            esp_println::println!("Declaring publisher on /chatter (std_msgs/Int32) over DDS");
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
