//! nano-ros Zephyr Service Server Example (Rust)
//!
//! A ROS 2 compatible service server running on Zephyr RTOS using the nano-ros API.
//! The server responds to AddTwoInts service requests.

#![no_std]

use log::{error, info};
use nano_ros::{ShimExecutor, ShimNodeError};
use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nano-ros Zephyr Service Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), ShimNodeError> {
    let mut executor = ShimExecutor::new(b"tcp/192.0.2.2:7447\0")?;
    let mut node = executor.create_node("add_two_ints_server")?;
    let mut service = node.create_service::<AddTwoInts>("/add_two_ints")?;

    info!("Service server ready: /add_two_ints");
    info!("Waiting for service requests...");

    loop {
        let _ = executor.spin_once(100);
        let _ = service.handle_request(|req| {
            let sum = req.a + req.b;
            info!("{} + {} = {}", req.a, req.b, sum);
            AddTwoIntsResponse { sum }
        });
    }
}
