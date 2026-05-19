//! Native DDS Service Client Example
//!
//! ROS 2 service client using nros with the DDS/RTPS backend.
//! Uses brokerless peer-to-peer discovery — no router or agent needed.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p native-dds-service-server
//!
//! # In another terminal:
//! cargo run -p native-dds-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_log::{Logger, nros_debug, nros_error, nros_info, nros_trace, nros_warn};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-client");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros DDS Service Client Example");
    nros_info!(&LOGGER, "================================");

    let config = ExecutorConfig::from_env().node_name("add_two_ints_client");
    // Phase 115.L.5 — install dust-dds C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_dds::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    let mut node = executor
        .create_node("add_two_ints_client")
        .expect("Failed to create node");
    nros_info!(&LOGGER, "Node created: add_two_ints_client");

    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");
    nros_info!(&LOGGER, "Service client created for: /add_two_ints");

    // Allow time for SPDP/SEDP discovery to complete (DDS service
    // discovery is slower than zenoh's broker-mediated routing).
    std::thread::sleep(std::time::Duration::from_secs(3));

    let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let request = AddTwoIntsRequest { a, b };
        nros_info!(&LOGGER, "Calling service: {} + {} = ?", a, b);

        let mut promise = match client.call(&request) {
            Ok(p) => p,
            Err(e) => {
                nros_error!(&LOGGER, "Failed to send request: {:?}", e);
                std::process::exit(1);
            }
        };

        let response = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
            Ok(reply) => reply,
            Err(e) => {
                nros_error!(&LOGGER, "Service call failed: {:?}", e);
                nros_error!(&LOGGER, "Make sure the service server is running:");
                nros_error!(&LOGGER, "  cargo run -p native-dds-service-server");
                std::process::exit(1);
            }
        };

        nros_info!(&LOGGER, "Response: {} + {} = {}", a, b, response.sum);
        assert_eq!(response.sum, a + b, "Sum mismatch!");

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    nros_info!(&LOGGER, "All service calls completed successfully!");
}
