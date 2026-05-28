//! nros Zephyr Listener Example (Rust) — Phase 168.3 collapsed shape.
//!
//! Single example, two RMW backends. Cargo features `rmw-zenoh` /
//! `rmw-xrce` (mutually exclusive) select the backend.

#![no_std]

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-cyclonedds"
)))]
compile_error!(
    "Exactly one rmw-* feature must be enabled (rmw-zenoh | rmw-xrce | rmw-cyclonedds)."
);

#[cfg(any(
    all(feature = "rmw-zenoh", feature = "rmw-xrce"),
    all(feature = "rmw-zenoh", feature = "rmw-cyclonedds"),
    all(feature = "rmw-xrce", feature = "rmw-cyclonedds"),
))]
compile_error!("rmw-zenoh / rmw-xrce / rmw-cyclonedds are mutually exclusive.");

use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};
use std_msgs::msg::Int32;

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?;
    }
    Ok(())
}

#[cfg(feature = "rmw-zenoh")]
fn make_config() -> ExecutorConfig<'static> {
    ExecutorConfig::new("tcp/127.0.0.1:7456")
}

#[cfg(feature = "rmw-cyclonedds")]
fn make_config() -> ExecutorConfig<'static> {
    // Domain from Kconfig (CONFIG_NROS_DOMAIN_ID) — compile-time, embedded-style.
    // Test fixtures build distinct domains per role-set via -DCONFIG_NROS_DOMAIN_ID
    // so the native_sim Cyclone tests run in parallel (distinct RTPS ports).
    ExecutorConfig::new("")
        .domain_id(zephyr::kconfig::CONFIG_NROS_DOMAIN_ID as u32)
        .node_name("cyclonedds_listener")
}

#[cfg(feature = "rmw-xrce")]
fn make_config() -> ExecutorConfig<'static> {
    use core::fmt::Write;
    static mut LOCATOR: heapless::String<48> = heapless::String::new();
    unsafe {
        LOCATOR.clear();
        let _ = write!(
            LOCATOR,
            "{}:{}",
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR,
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT
        );
        let s: &'static str = core::str::from_utf8_unchecked(LOCATOR.as_bytes());
        ExecutorConfig::new(s).node_name("xrce_listener")
    }
}

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }
    info!("nros Zephyr Listener");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);
    register_rmw().expect("Failed to register RMW backend");

    let config = make_config();
    let mut executor: Executor = Executor::open(&config)?;
    let nid = executor.node_builder("listener").build()?;

    let mut count: u32 = 0;
    executor
        .node_mut(nid)
        .create_subscription::<Int32, _>("/chatter", move |msg: &Int32| {
            count += 1;
            info!("Received[{}]: {}", count, msg.data);
        })?;

    info!("Waiting for messages on /chatter...");
    executor.spin(core::time::Duration::from_millis(1000));
}
