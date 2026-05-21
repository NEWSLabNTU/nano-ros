//! Native service-server example — shared logic for both build paths
//! (Phase 170.A). `run()` is shared by the pure-cargo `fn main()`
//! (zenoh/xrce) and the Cyclone DDS `rust_main()` C entry.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::prelude::*;

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!("this example requires exactly one of `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce`",);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?;
    }
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    Ok(())
}

/// Service-server body — serve `AddTwoInts` on `/add_two_ints`.
pub fn run() {
    info!("nros Service Server Example");
    info!("================================");

    register_rmw().expect("Failed to register RMW backend");
    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");
    info!("Node created: add_two_ints_server");

    executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            info!("Received request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    info!("Service server created: /add_two_ints");
    info!("Waiting for service requests...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}

#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    env_logger::init();
    run();
    0
}

#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_platform_cffi as _;
