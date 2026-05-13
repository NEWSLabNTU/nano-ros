//! nros Zephyr XRCE Talker Example (Rust)
//!
//! A ROS 2 compatible publisher running on Zephyr RTOS using the XRCE-DDS backend.
//! Uses the timer+spin pattern: registers a timer callback that publishes
//! messages at 1 Hz, then spins the executor.

#![no_std]

use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError, TimerDuration};
use std_msgs::msg::Int32;

#[no_mangle]
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
    // Phase 120.2: locator from Kconfig (CONFIG_NROS_XRCE_AGENT_ADDR/PORT)
    // so test fixtures can override the port per (variant, lang).
    use core::fmt::Write;
    let mut locator: heapless::String<48> = heapless::String::new();
    let _ = write!(
        locator,
        "{}:{}",
        zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR,
        zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT
    );
    let config = ExecutorConfig::new(&locator).node_name("xrce_talker");
    // Phase 115.L.x — install C-vtable backend before session open.
    let mut executor: Executor = Executor::open(&config)?;

    let mut node = executor.create_node("xrce_talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    let mut counter: i32 = 0;
    executor.register_timer(TimerDuration::from_millis(1000), move || {
        let _ = publisher.publish(&Int32 { data: counter });
        info!("Published: {}", counter);
        counter = counter.wrapping_add(1);
    })?;

    info!("Publishing messages...");

    executor.spin(core::time::Duration::from_millis(10));
}
