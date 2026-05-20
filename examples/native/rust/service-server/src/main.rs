//! Native Service Server Example
//!
//! Demonstrates a ROS 2 service server using nros with the Executor API.
//! Uses callback-based service handling via spin_blocking().
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Run the service server:
//! cargo run -p native-rs-service-server
//!
//! # In another terminal, run the client:
//! cargo run -p native-rs-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::prelude::*;

// Phase 118 — RMW selection is build-time via mutually exclusive
// `rmw-{zenoh,cyclonedds,xrce}` features. `register_rmw()` fans out under
// `#[cfg(feature)]`; the rest of the file stays RMW-agnostic.

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

fn main() {
    env_logger::init();

    info!("nros Service Server Example");
    info!("================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    // Phase 115.L.5 — install zenoh-pico C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    register_rmw().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    info!("Node created: add_two_ints_server");

    // Register service callback
    executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            info!("Received request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    info!("Service server created: /add_two_ints");

    info!("Waiting for service requests...");
    info!("(Run native-rs-service-client in another terminal)");

    // Blocking spin loop
    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
