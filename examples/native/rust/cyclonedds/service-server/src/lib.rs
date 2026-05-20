//! Native Rust service server over the Cyclone DDS RMW backend.
//!
//! Phase 171.C.1.rust — native rust cyclonedds is cmake-driven: this
//! crate compiles to a `staticlib` named `rustapp` exposing a C
//! `rust_main()` entry. The per-example `CMakeLists.txt` runs
//! `nros_generate_interfaces(example_interfaces)` (emits the Cyclone
//! IDL typesupport via idlc), builds the C++ `nros-rmw-cyclonedds`
//! backend, and links both alongside this staticlib + `libddsc` +
//! `stdc++`. A tiny `src/main.c` calls `rust_main()`.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_log::{nros_error, nros_info, Logger};

static LOGGER: Logger = Logger::new("service-server");

// Pull the POSIX C platform port into the link graph so
// `nros_platform_*` resolve.
extern crate nros_platform_cffi as _;

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    if nros_rmw_cyclonedds_sys::register().is_err() {
        nros_error!(&LOGGER, "Failed to register Cyclone DDS RMW backend");
        return 1;
    }
    nros_info!(&LOGGER, "nros Native Service Server (Cyclone DDS Transport)");

    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    let mut executor: Executor = match Executor::open(&config) {
        Ok(e) => e,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to open executor");
            return 1;
        }
    };

    if executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            nros_info!(&LOGGER, "Received request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })
        .is_err()
    {
        nros_error!(&LOGGER, "Failed to create service");
        return 1;
    }
    nros_info!(&LOGGER, "Service server created: /add_two_ints");
    nros_info!(&LOGGER, "Waiting for service requests...");

    match executor.spin_blocking(SpinOptions::default()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
