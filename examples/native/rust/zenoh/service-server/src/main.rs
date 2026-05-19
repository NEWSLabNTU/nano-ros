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
use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};
use nros::prelude::*;

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-server");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros Service Server Example");
    nros_info!(&LOGGER, "================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    // Phase 115.L.5 — install zenoh-pico C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    nros_info!(&LOGGER, "Node created: add_two_ints_server");

    // Register service callback
    executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            nros_info!(&LOGGER, "Received request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    nros_info!(&LOGGER, "Service server created: /add_two_ints");

    nros_info!(&LOGGER, "Waiting for service requests...");
    nros_info!(&LOGGER, "(Run native-rs-service-client in another terminal)");

    // Blocking spin loop
    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        nros_error!(&LOGGER, "Spin error: {:?}", e);
    }
}
