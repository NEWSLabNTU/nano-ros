//! nros Zephyr Talker Example (Rust) — Phase 168.1 collapsed shape.
//!
//! Single example, three RMW backends. Cargo features
//! `rmw-zenoh` / `rmw-xrce` (mutually exclusive)
//! select the backend at build time; CMakeLists.txt maps Kconfig
//! `CONFIG_NROS_RMW_<X>=y` to the matching feature.

#![no_std]

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cyclonedds")))]
compile_error!("Exactly one rmw-* feature must be enabled (rmw-zenoh | rmw-xrce | rmw-cyclonedds).");

#[cfg(any(
    all(feature = "rmw-zenoh", feature = "rmw-xrce"),
    all(feature = "rmw-zenoh", feature = "rmw-cyclonedds"),
    all(feature = "rmw-xrce", feature = "rmw-cyclonedds"),
))]
compile_error!("rmw-zenoh / rmw-xrce / rmw-cyclonedds are mutually exclusive.");

use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError, TimerDuration};
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
    ExecutorConfig::new("").domain_id(0).node_name("cyclonedds_talker")
}

#[cfg(feature = "rmw-xrce")]
fn make_config() -> ExecutorConfig<'static> {
    use core::fmt::Write;
    // Phase 120.2 — locator built from Kconfig at runtime so test
    // fixtures can override the port per (variant, lang).
    static mut LOCATOR: heapless::String<48> = heapless::String::new();
    unsafe {
        LOCATOR.clear();
        let _ = write!(
            LOCATOR,
            "{}:{}",
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR,
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT
        );
        // LOCATOR is the only writer — borrow it as &'static str.
        let s: &'static str = core::str::from_utf8_unchecked(LOCATOR.as_bytes());
        ExecutorConfig::new(s).node_name("xrce_talker")
    }
}

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Talker");
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

    let mut node = executor.create_node("talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    let mut counter: i32 = 0;
    executor.register_timer(TimerDuration::from_millis(1000), move || {
        let _ = publisher.publish(&Int32 { data: counter });
        info!("Published: {}", counter);
        counter = counter.wrapping_add(1);
    })?;

    info!("Publishing messages...");

    executor.spin(core::time::Duration::from_millis(10));
}
