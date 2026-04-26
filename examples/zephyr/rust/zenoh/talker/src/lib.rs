//! nros Zephyr Talker Example (Rust)
//!
//! A ROS 2 compatible publisher running on Zephyr RTOS using the nros API.
//! Uses the timer+spin pattern: registers a timer callback that publishes
//! messages at 1 Hz, then spins the executor.

#![no_std]

use log::{error, info};
use nros::{ExecutorConfig, Executor, NodeError, TimerDuration};
use std_msgs::msg::Int32;

#[no_mangle]
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

fn run() -> Result<(), NodeError> {
    // Wait for the Zephyr network interface to come up before opening the
    // zenoh session. Mirrors what the C/C++ examples do via
    // zpico_zephyr_wait_network() — required on native_sim where the TAP
    // link reports up asynchronously after IPv4 assignment.
    let _ = nros::platform::zephyr::wait_for_network(2000);

    let config = ExecutorConfig::new("tcp/127.0.0.1:7456");
    let mut executor: Executor = Executor::open(&config)?;

    let mut node = executor.create_node("talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    let mut counter: i32 = 0;
    executor.add_timer(TimerDuration::from_millis(1000), move || {
        let _ = publisher.publish(&Int32 { data: counter });
        info!("Published: {}", counter);
        counter = counter.wrapping_add(1);
    })?;

    info!("Publishing messages...");

    executor.spin(core::time::Duration::from_millis(10));
}
