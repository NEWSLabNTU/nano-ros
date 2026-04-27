//! nros Zephyr DDS Service Server Example (Rust)
//!
//! ROS 2 / DDS-RTPS service server running on Zephyr RTOS, driven by
//! the cooperative `NrosPlatformRuntime<ZephyrPlatform>`. No OS
//! threads, no router, no XRCE agent — speaks RTPS directly to peer
//! DDS participants on the same domain.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr DDS Service Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);

    // DDS uses domain_id, not a locator string. Pass an empty locator
    // and let dust-dds derive the RTPS port set from the domain id.
    let config = ExecutorConfig::new("")
        .domain_id(0)
        .node_name("dds_service_server");
    let mut executor: Executor = Executor::open(&config)?;

    executor.add_service::<AddTwoInts, _>("/add_two_ints", |req| {
        let sum = req.a + req.b;
        info!("{} + {} = {}", req.a, req.b, sum);
        AddTwoIntsResponse { sum }
    })?;

    info!("Service server ready: /add_two_ints");
    info!("Waiting for service requests...");

    executor.spin(core::time::Duration::from_millis(100));
}
