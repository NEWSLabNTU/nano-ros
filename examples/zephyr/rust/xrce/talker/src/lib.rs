//! nros Zephyr XRCE Talker Example (Rust)
//!
//! A ROS 2 compatible publisher running on Zephyr RTOS using the XRCE-DDS backend.

#![no_std]

use log::{error, info};
use nros::{EmbeddedConfig, EmbeddedExecutor, EmbeddedNodeError};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr XRCE Talker");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), EmbeddedNodeError> {
    // The locator for XRCE is "agent_addr:port"
    let config = EmbeddedConfig::new("192.0.2.2:2018");
    let mut executor = EmbeddedExecutor::open(&config)?;
    let mut node = executor.create_node("xrce_talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    info!("Publishing messages...");

    let mut counter: i32 = 0;

    loop {
        publisher.publish(&Int32 { data: counter })?;
        info!("[{}] Published: data={}", counter, counter);
        counter = counter.wrapping_add(1);
        let _ = executor.drive_io(1000);
    }
}
