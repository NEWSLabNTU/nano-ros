//! nros Zephyr XRCE Service Server Example (Rust)
//!
//! A ROS 2 compatible service server running on Zephyr RTOS using the
//! XRCE-DDS backend. Uses the callback+spin pattern for request handling.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr XRCE Service Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    let _ = nros::platform::zephyr::wait_for_network(2000);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    // XRCE locator: "agent_addr:port" (no tcp/ prefix). Agent is the
    // MicroXRCEAgent process on the host (default port 2018).
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
    let config = ExecutorConfig::new(&locator).node_name("xrce_service_server");
    // Phase 104.A — bare-metal callers explicitly register the RMW
    // backend before `Executor::open`. POSIX hosts auto-register via
    // `.init_array`; this target doesn't walk that section.
    nros_rmw_xrce_cffi::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config)?;

    executor.register_service::<AddTwoInts, _>("/add_two_ints", |req| {
        let sum = req.a + req.b;
        info!("{} + {} = {}", req.a, req.b, sum);
        AddTwoIntsResponse { sum }
    })?;

    info!("Service server ready: /add_two_ints");
    info!("Waiting for service requests...");

    executor.spin(core::time::Duration::from_millis(100));
}
