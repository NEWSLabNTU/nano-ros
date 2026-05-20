//! Native Rust talker over the Cyclone DDS RMW backend.
//!
//! Phase 171.C.1.rust — native rust cyclonedds is cmake-driven: this
//! crate compiles to a `staticlib` named `rustapp` exposing a C
//! `rust_main()` entry. The per-example `CMakeLists.txt` runs
//! `nros_generate_interfaces(std_msgs)` (emits the Cyclone IDL
//! typesupport via idlc), builds the C++ `nros-rmw-cyclonedds`
//! backend, and links both alongside this staticlib + `libddsc` +
//! `stdc++`. A tiny `src/main.c` calls `rust_main()`.
//!
//! The Rust `nros` runtime owns the `nros-rmw-cffi` registry; the C++
//! backend's `nros_rmw_cyclonedds_register()` writes the cyclonedds
//! vtable into THAT registry (the `#[no_mangle]` REGISTRY static
//! collapses cross-language, Phase 134.fix).

use nros::prelude::*;
use nros_log::{nros_error, nros_info, Logger};
use std_msgs::msg::Int32;

static LOGGER: Logger = Logger::new("talker");

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
    nros_info!(&LOGGER, "nros Native Talker (Cyclone DDS Transport)");

    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor: Executor = match Executor::open(&config) {
        Ok(e) => e,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to open executor");
            return 1;
        }
    };

    let publisher = {
        let mut node = match executor.create_node("talker") {
            Ok(n) => n,
            Err(_) => return 1,
        };
        nros_info!(&LOGGER, "Node created: talker");
        match node.create_publisher::<Int32>("/chatter") {
            Ok(p) => p,
            Err(_) => return 1,
        }
    };
    nros_info!(&LOGGER, "Publisher created for topic: /chatter");

    let mut count: i32 = 0;
    if executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            match publisher.publish(&Int32 { data: count }) {
                Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                Err(e) => nros_error!(&LOGGER, "Publish error: {:?}", e),
            }
            count = count.wrapping_add(1);
        })
        .is_err()
    {
        return 1;
    }
    nros_info!(&LOGGER, "Publishing Int32 messages every 1s...");

    match executor.spin_blocking(SpinOptions::default()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
