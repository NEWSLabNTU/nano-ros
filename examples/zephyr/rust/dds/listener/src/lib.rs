//! nros Zephyr DDS Listener Example (Rust) — Phase 71.8
//!
//! ROS 2 / DDS-RTPS subscriber running on Zephyr RTOS, driven by the
//! cooperative `NrosPlatformRuntime<ZephyrPlatform>` plus
//! `NrosUdpTransportFactory`.

#![no_std]

use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};
use std_msgs::msg::Int32;

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr DDS Listener");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);

    let config = ExecutorConfig::new("").domain_id(0).node_name("listener");
    let mut executor: Executor = Executor::open(&config)?;

    let mut count: u32 = 0;
    executor.add_subscription::<Int32, _>("/chatter", move |msg: &Int32| {
        count += 1;
        info!("Received: {}", msg.data);
    })?;

    info!("Waiting for messages on /chatter...");

    executor.spin(core::time::Duration::from_millis(1000));
}
