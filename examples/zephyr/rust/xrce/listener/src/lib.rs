//! nros Zephyr XRCE Listener Example (Rust)
//!
//! A ROS 2 compatible subscriber running on Zephyr RTOS using the XRCE-DDS backend.
//! Uses the callback+spin pattern for message reception.

#![no_std]

use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};
use std_msgs::msg::Int32;

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr XRCE Listener");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    let _ = nros::platform::zephyr::wait_for_network(2000);

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
    let config = ExecutorConfig::new(&locator).node_name("xrce_listener");
    // Phase 104.A — bare-metal callers explicitly register the RMW
    // backend before `Executor::open`. POSIX hosts auto-register via
    // `.init_array`; this target doesn't walk that section.
    nros_rmw_xrce_cffi::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config)?;

    let mut count: u32 = 0;
    executor.register_subscription::<Int32, _>("/chatter", move |msg: &Int32| {
        count += 1;
        info!("Received: {}", msg.data);
    })?;

    info!("Waiting for messages on /chatter...");

    executor.spin(core::time::Duration::from_millis(1000));
}
