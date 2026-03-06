//! nros Zephyr XRCE Talker Example (Rust)
//!
//! A ROS 2 compatible publisher running on Zephyr RTOS using the XRCE-DDS backend.
//! Uses the timer+spin pattern: registers a timer callback that publishes
//! messages at 1 Hz, then spins the executor.

#![no_std]

use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError, TimerDuration};
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

fn run() -> Result<(), NodeError> {
    // The locator for XRCE is "agent_addr:port" (no tcp/ prefix)
    let config = ExecutorConfig::new("192.0.2.2:2018");
    let mut executor: Executor<_> = Executor::open(&config)?;

    let mut node = executor.create_node("xrce_talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    let mut counter: i32 = 0;
    executor.add_timer(TimerDuration::from_millis(1000), move || {
        let _ = publisher.publish(&Int32 { data: counter });
        info!("[{}] Published: data={}", counter, counter);
        counter = counter.wrapping_add(1);
    })?;

    info!("Publishing messages...");

    executor.spin(10);
}
