//! nros Zephyr Service Server Example (Rust)
//!
//! A ROS 2 compatible service server running on Zephyr RTOS using the nros API.
//! Uses the callback+spin pattern for request handling.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::{ExecutorConfig, Executor, NodeError};

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Service Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    // Wait for the Zephyr network interface to come up before opening the
    // zenoh session. Required on native_sim where the TAP link reports up
    // asynchronously after IPv4 assignment.
    let _ = nros::platform::zephyr::wait_for_network(2000);

    let config = ExecutorConfig::new("tcp/192.0.2.2:7456");
    let mut executor: Executor = Executor::open(&config)?;

    executor.add_service::<AddTwoInts, _>("/add_two_ints", |req| {
        let sum = req.a + req.b;
        info!("{} + {} = {}", req.a, req.b, sum);
        AddTwoIntsResponse { sum }
    })?;

    info!("Service server ready: /add_two_ints");
    info!("Waiting for service requests...");

    executor.spin(100);
}
