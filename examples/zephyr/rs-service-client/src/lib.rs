//! nros Zephyr Service Client Example (Rust)
//!
//! A ROS 2 compatible service client running on Zephyr RTOS using the nros API.
//! The client sends AddTwoInts service requests.

#![no_std]

use log::{error, info};
use nros::{ShimExecutor, ShimNodeError};
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Service Client");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), ShimNodeError> {
    let mut executor = ShimExecutor::new(b"tcp/192.0.2.2:7447\0")?;
    let mut node = executor.create_node("add_two_ints_client")?;
    let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;

    info!("Service client ready: /add_two_ints");
    info!("Sending service requests...");

    // Allow time for connection to stabilize
    zephyr::time::sleep(zephyr::time::Duration::secs(2));

    let mut count: i64 = 0;

    loop {
        let req = AddTwoIntsRequest {
            a: count,
            b: count + 1,
        };
        info!("[{}] Sending: {} + {}", count, req.a, req.b);

        match client.call(&req) {
            Ok(resp) => info!("[{}] Response: sum={}", count, resp.sum),
            Err(e) => error!("[{}] Call failed: {:?}", count, e),
        }

        count += 1;
        zephyr::time::sleep(zephyr::time::Duration::secs(2));
    }
}
