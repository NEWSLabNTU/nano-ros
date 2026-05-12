//! nros Zephyr XRCE Service Client Example (Rust)
//!
//! A ROS 2 compatible service client running on Zephyr RTOS using the
//! XRCE-DDS backend. Uses the Promise API: `client.call()` returns
//! immediately, then `promise.wait()` drives I/O and waits for the reply.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr XRCE Service Client");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    // XRCE locator: "agent_addr:port" (no tcp/ prefix).
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
    let config = ExecutorConfig::new(&locator).node_name("xrce_service_client");
    // Phase 115.L.x — install C-vtable backend before session open.
    nros_rmw_xrce_cffi::register().expect("xrce RMW register failed");
    let mut executor = Executor::open(&config)?;

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

        let mut promise = client.call(&req)?;

        match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
            Ok(resp) => {
                info!("[{}] Response: sum={}", count, resp.sum);
            }
            Err(e) => {
                error!("[{}] Call failed: {:?}", count, e);
            }
        }

        count += 1;
        zephyr::time::sleep(zephyr::time::Duration::secs(2));
    }
}
