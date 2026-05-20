//! Native Rust listener over the Cyclone DDS RMW backend.
//!
//! Phase 171.C.1.rust — native rust cyclonedds is cmake-driven: this
//! crate compiles to a `staticlib` named `rustapp` exposing a C
//! `rust_main()` entry. The per-example `CMakeLists.txt` runs
//! `nros_generate_interfaces(std_msgs)` (emits the Cyclone IDL
//! typesupport via idlc), builds the C++ `nros-rmw-cyclonedds`
//! backend, and links both alongside this staticlib + `libddsc` +
//! `stdc++`. A tiny `src/main.c` calls `rust_main()`.

use nros::prelude::*;
use nros_log::{nros_error, nros_info, Logger};
use std_msgs::msg::Int32;

static LOGGER: Logger = Logger::new("listener");

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
    nros_info!(&LOGGER, "nros Native Listener (Cyclone DDS Transport)");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor: Executor = match Executor::open(&config) {
        Ok(e) => e,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to open executor");
            return 1;
        }
    };

    if executor
        .register_subscription::<Int32, _>("/chatter", move |msg: &Int32| {
            nros_info!(&LOGGER, "Received: {}", msg.data);
        })
        .is_err()
    {
        nros_error!(&LOGGER, "Failed to create subscription");
        return 1;
    }
    nros_info!(&LOGGER, "Subscriber created for topic: /chatter");
    nros_info!(&LOGGER, "Waiting for Int32 messages on /chatter...");

    match executor.spin_blocking(SpinOptions::default()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
