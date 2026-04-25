//! nros Zephyr DDS Talker Example (Rust) — Phase 71.8
//!
//! ROS 2 / DDS-RTPS publisher running on Zephyr RTOS, driven by the
//! cooperative `NrosPlatformRuntime<ZephyrPlatform>` plus
//! `NrosUdpTransportFactory` (Phase 71.2/71.3). No OS threads, no
//! router, no XRCE agent — speaks RTPS directly to peer DDS
//! participants on the same domain.

#![no_std]

use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError, TimerDuration};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr DDS Talker");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    // Wait for the network interface to come up before opening the DDS
    // session — `NrosUdpTransportFactory::create_participant` binds the
    // RTPS PSM ports immediately, which fails if the interface isn't
    // ready yet on native_sim.
    let _ = nros::platform::zephyr::wait_for_network(2000);

    // DDS uses domain_id, not a locator string. Pass an empty locator
    // and let dust-dds derive the RTPS port set from the domain id.
    let config = ExecutorConfig::new("").domain_id(0).node_name("talker");
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
