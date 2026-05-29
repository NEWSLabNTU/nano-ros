//! nros Zephyr Service Server (Rust) — Phase 168.3 collapsed shape.

#![no_std]

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cyclonedds")))]
compile_error!("Exactly one rmw-* feature must be enabled.");

#[cfg(any(
    all(feature = "rmw-zenoh", feature = "rmw-xrce"),
    all(feature = "rmw-zenoh", feature = "rmw-cyclonedds"),
    all(feature = "rmw-xrce", feature = "rmw-cyclonedds"),
))]
compile_error!("rmw-zenoh / rmw-xrce / rmw-cyclonedds are mutually exclusive.");

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    { nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?; }
    #[cfg(feature = "rmw-xrce")]
    { nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?; }
    #[cfg(feature = "rmw-cyclonedds")]
    { nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?; }
    Ok(())
}

#[cfg(feature = "rmw-zenoh")]
fn make_config() -> ExecutorConfig<'static> {
    ExecutorConfig::new("tcp/127.0.0.1:7466")
}

#[cfg(feature = "rmw-cyclonedds")]
fn make_config() -> ExecutorConfig<'static> {
    // Domain from Kconfig (CONFIG_NROS_DOMAIN_ID) — compile-time, embedded-style.
    // Test fixtures build distinct domains per role-set via -DCONFIG_NROS_DOMAIN_ID
    // so the native_sim Cyclone tests run in parallel (distinct RTPS ports).
    ExecutorConfig::new("")
        .domain_id(zephyr::kconfig::CONFIG_NROS_DOMAIN_ID as u32)
        .node_name("cyclonedds_service_server")
}

#[cfg(feature = "rmw-xrce")]
fn make_config() -> ExecutorConfig<'static> {
    use core::fmt::Write;
    static mut LOCATOR: heapless::String<48> = heapless::String::new();
    // SAFETY: single-threaded startup; this is the sole accessor of the
    // `LOCATOR` static, and `from_utf8_unchecked` is fed bytes written here
    // from formatted Kconfig string values (valid UTF-8).
    unsafe {
        LOCATOR.clear();
        let _ = write!(
            LOCATOR,
            "{}:{}",
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR,
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT
        );
        let s: &'static str = core::str::from_utf8_unchecked(LOCATOR.as_bytes());
        ExecutorConfig::new(s).node_name("xrce_service_server")
    }
}

#[no_mangle]
extern "C" fn rust_main() {
    // SAFETY: installs the logger once during single-threaded startup, before any logging call.
    unsafe { zephyr::set_logger().ok(); }
    info!("nros Zephyr Service Server");
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

    executor.register_service::<AddTwoInts, _>("/add_two_ints", |req| {
        let sum = req.a + req.b;
        info!("{} + {} = {}", req.a, req.b, sum);
        AddTwoIntsResponse { sum }
    })?;

    info!("Service server ready: /add_two_ints");
    executor.spin(core::time::Duration::from_millis(100));
}
