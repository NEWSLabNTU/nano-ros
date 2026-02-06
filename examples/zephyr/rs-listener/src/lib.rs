//! nano-ros Zephyr Listener Example (Rust)
//!
//! A ROS 2 compatible subscriber running on Zephyr RTOS using the nano-ros API.

#![no_std]

use log::{error, info};
use nano_ros::{ShimExecutor, ShimNodeError};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nano-ros Zephyr Listener");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), ShimNodeError> {
    let mut executor = ShimExecutor::new(b"tcp/192.0.2.2:7447\0")?;
    let mut node = executor.create_node("listener")?;
    let mut subscription = node.create_subscription::<Int32>("/chatter")?;

    info!("Waiting for messages on /chatter...");

    let mut count: u32 = 0;

    loop {
        let _ = executor.spin_once(1000);
        while let Ok(Some(msg)) = subscription.try_recv() {
            count += 1;
            info!("[{}] Received: data={}", count, msg.data);
        }
    }
}
