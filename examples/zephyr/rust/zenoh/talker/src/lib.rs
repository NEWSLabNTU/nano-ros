//! nros Zephyr Talker Example (Rust)
//!
//! A ROS 2 compatible publisher running on Zephyr RTOS using the nros API.

#![no_std]

use log::{error, info};
use nros::{ShimExecutor, ShimNodeError};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Talker");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), ShimNodeError> {
    let mut executor = ShimExecutor::new(b"tcp/192.0.2.2:7447\0")?;
    let mut node = executor.create_node("talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    info!("Publishing messages...");

    let mut counter: i32 = 0;

    loop {
        publisher.publish(&Int32 { data: counter })?;
        info!("[{}] Published: data={}", counter, counter);
        counter = counter.wrapping_add(1);
        let _ = executor.spin_once(1000);
    }
}
