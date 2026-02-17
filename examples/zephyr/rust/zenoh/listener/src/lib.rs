//! nros Zephyr Listener Example (Rust)
//!
//! A ROS 2 compatible subscriber running on Zephyr RTOS using the nros API.

#![no_std]

use log::{error, info};
use nros::{EmbeddedConfig, EmbeddedExecutor, EmbeddedNodeError};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Listener");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), EmbeddedNodeError> {
    let config = EmbeddedConfig::new("tcp/192.0.2.2:7447");
    let mut executor = EmbeddedExecutor::open(&config)?;
    let mut node = executor.create_node("listener")?;
    let mut subscription = node.create_subscription::<Int32>("/chatter")?;

    info!("Waiting for messages on /chatter...");

    let mut count: u32 = 0;

    loop {
        let _ = executor.drive_io(1000);
        while let Ok(Some(msg)) = subscription.try_recv() {
            count += 1;
            info!("[{}] Received: data={}", count, msg.data);
        }
    }
}
