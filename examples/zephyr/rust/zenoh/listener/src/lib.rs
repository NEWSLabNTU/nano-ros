//! nros Zephyr Listener Example (Rust)
//!
//! A ROS 2 compatible subscriber running on Zephyr RTOS using the nros API.
//! Uses the callback+spin pattern for message reception.

#![no_std]

use log::{error, info};
use nros::{ExecutorConfig, Executor, NodeError};
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

fn run() -> Result<(), NodeError> {
    let config = ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut executor: Executor<_> = Executor::open(&config)?;

    let mut count: u32 = 0;
    executor.add_subscription::<Int32, _>("/chatter", move |msg: &Int32| {
        count += 1;
        info!("[{}] Received: data={}", count, msg.data);
    })?;

    info!("Waiting for messages on /chatter...");

    executor.spin(1000);
}
