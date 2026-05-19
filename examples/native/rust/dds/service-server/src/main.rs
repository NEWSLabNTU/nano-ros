//! Native DDS Service Server Example
//!
//! ROS 2 service server using nros with the DDS/RTPS backend.
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

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_log::{Logger, nros_debug, nros_error, nros_info, nros_trace, nros_warn};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-server");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros DDS Service Server Example");
    nros_info!(&LOGGER, "================================");

    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    // Phase 115.L.5 — install dust-dds C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_dds::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    nros_info!(&LOGGER, "Node created: add_two_ints_server");

    executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            nros_info!(
                &LOGGER,
                "Received request: {} + {} = {}",
                request.a,
                request.b,
                sum
            );
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    nros_info!(&LOGGER, "Service server created: /add_two_ints");

    nros_info!(&LOGGER, "Waiting for service requests...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        nros_error!(&LOGGER, "Spin error: {:?}", e);
    }
}
