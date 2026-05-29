//! nros Zephyr Service Client (Rust) — Phase 168.3 collapsed shape.

#![no_std]

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cyclonedds")))]
compile_error!("Exactly one rmw-* feature must be enabled (rmw-zenoh | rmw-xrce | rmw-cyclonedds).");

#[cfg(any(
    all(feature = "rmw-zenoh", feature = "rmw-xrce"),
    all(feature = "rmw-zenoh", feature = "rmw-cyclonedds"),
    all(feature = "rmw-xrce", feature = "rmw-cyclonedds"),
))]
compile_error!("rmw-zenoh / rmw-xrce / rmw-cyclonedds are mutually exclusive.");

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
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
        .node_name("cyclonedds_service_client")
}

#[cfg(feature = "rmw-xrce")]
fn make_config() -> ExecutorConfig<'static> {
    use core::fmt::Write;
    static mut LOCATOR: heapless::String<48> = heapless::String::new();
    // SAFETY: single-threaded startup; this is the sole accessor of the
    // `LOCATOR` static, and `from_utf8_unchecked` is fed bytes written here
    // from formatted Kconfig string values (valid UTF-8).
    unsafe {
        let loc = core::ptr::addr_of_mut!(LOCATOR);
        (*loc).clear();
        let _ = write!(
            *loc,
            "{}:{}",
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR,
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT
        );
        let s: &'static str = core::str::from_utf8_unchecked((*loc).as_bytes());
        ExecutorConfig::new(s).node_name("xrce_service_client")
    }
}

// 177.39 — generous per-call budget. `promise.wait` spins the executor for the
// whole duration, which is what drives Cyclone's RELIABLE+VOLATILE service
// discovery/match (the request is buffered until the request-writer matches the
// server's reader, Phase 171.0.a). Under native_sim CPU contention that
// match + roundtrip can exceed a few seconds, so a 5 s cap timed out (177.39).
// The C client tolerates this via a blocking `nros_client_call`; match that
// tolerance here. Harmless for zenoh (returns early on the fast reply).
const CALL_TIMEOUT_MS: u64 = 15_000;

#[no_mangle]
extern "C" fn rust_main() {
    // SAFETY: installs the logger once during single-threaded startup, before any logging call.
    unsafe { zephyr::set_logger().ok(); }
    info!("nros Zephyr Service Client");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);
    register_rmw().expect("Failed to register RMW backend");

    let config = make_config();
    let mut executor = Executor::open(&config)?;

    let mut node = executor.create_node("add_two_ints_client")?;
    let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;

    info!("Service client ready: /add_two_ints");
    zephyr::time::sleep(zephyr::time::Duration::secs(2));

    let mut count: i64 = 0;
    loop {
        let req = AddTwoIntsRequest { a: count, b: count + 1 };
        info!("[{}] Sending: {} + {}", count, req.a, req.b);
        let mut promise = client.call(&req)?;
        match promise.wait(&mut executor, core::time::Duration::from_millis(CALL_TIMEOUT_MS)) {
            Ok(resp) => info!("[{}] Response: sum={}", count, resp.sum),
            Err(e) => error!("[{}] Call failed: {:?}", count, e),
        }
        count += 1;
        zephyr::time::sleep(zephyr::time::Duration::secs(2));
    }
}
