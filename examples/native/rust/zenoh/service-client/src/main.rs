//! Native Service Client Example
//!
//! Demonstrates a ROS 2 service client using nros with the Promise API.
//! The client sends a request (non-blocking), then waits for the reply
//! using `promise.wait()` which drives I/O internally.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Start the service server:
//! cargo run -p native-rs-service-server
//!
//! # Run the client:
//! cargo run -p native-rs-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};
use nros::prelude::*;

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-client");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros Service Client Example");
    nros_info!(&LOGGER, "================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("add_two_ints_client");
    // Phase 115.L.5 — install zenoh-pico C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    // Create node and service client
    let mut node = executor
        .create_node("add_two_ints_client")
        .expect("Failed to create node");
    nros_info!(&LOGGER, "Node created: add_two_ints_client");

    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");
    nros_info!(&LOGGER, "Service client created for: /add_two_ints");

    match client.wait_for_service(&mut executor, core::time::Duration::from_secs(5)) {
        Ok(true) => nros_info!(&LOGGER, "Service server is available"),
        Ok(false) => {
            nros_error!(&LOGGER, "Timed out waiting for /add_two_ints service");
            std::process::exit(1);
        }
        Err(e) => {
            nros_error!(&LOGGER, "Failed while waiting for service: {:?}", e);
            std::process::exit(1);
        }
    }

    // Make several service calls using the Promise pattern
    let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let request = AddTwoIntsRequest { a, b };
        nros_info!(&LOGGER, "Calling service: {} + {} = ?", a, b);

        // Non-blocking: send request and get a promise
        let mut promise = match client.call(&request) {
            Ok(p) => p,
            Err(e) => {
                nros_error!(&LOGGER, "Failed to send request: {:?}", e);
                std::process::exit(1);
            }
        };

        // Wait for the reply (drives I/O internally)
        let response = match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
            Ok(reply) => reply,
            Err(e) => {
                nros_error!(&LOGGER, "Service call failed: {:?}", e);
                nros_error!(&LOGGER, "Make sure the service server is running:");
                nros_error!(&LOGGER, "  cargo run -p native-rs-service-server");
                std::process::exit(1);
            }
        };

        nros_info!(&LOGGER, "Response: {} + {} = {}", a, b, response.sum);
        assert_eq!(response.sum, a + b, "Sum mismatch!");

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    nros_info!(&LOGGER, "All service calls completed successfully!");
}
