//! Native Rust service client over the Cyclone DDS RMW backend.
//!
//! Phase 171.C.1.rust — native rust cyclonedds is cmake-driven: this
//! crate compiles to a `staticlib` named `rustapp` exposing a C
//! `rust_main()` entry. The per-example `CMakeLists.txt` runs
//! `nros_generate_interfaces(example_interfaces)` (emits the Cyclone
//! IDL typesupport via idlc), builds the C++ `nros-rmw-cyclonedds`
//! backend, and links both alongside this staticlib + `libddsc` +
//! `stdc++`. A tiny `src/main.c` calls `rust_main()`.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_log::{nros_error, nros_info, Logger};

static LOGGER: Logger = Logger::new("service-client");

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
    nros_info!(&LOGGER, "nros Native Service Client (Cyclone DDS Transport)");

    let config = ExecutorConfig::from_env().node_name("add_two_ints_client");
    let mut executor: Executor = match Executor::open(&config) {
        Ok(e) => e,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to open executor");
            return 1;
        }
    };

    let mut node = match executor.create_node("add_two_ints_client") {
        Ok(n) => n,
        Err(_) => return 1,
    };
    let mut client = match node.create_client::<AddTwoInts>("/add_two_ints") {
        Ok(c) => c,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to create client");
            return 1;
        }
    };
    nros_info!(&LOGGER, "Service client created for: /add_two_ints");

    match client.wait_for_service(&mut executor, core::time::Duration::from_secs(5)) {
        Ok(true) => nros_info!(&LOGGER, "Service server is available"),
        _ => {
            nros_error!(&LOGGER, "Timed out waiting for /add_two_ints service");
            return 1;
        }
    }

    let mut ok = 0;
    for (a, b) in [(5, 3), (10, 20), (100, 200), (-5, 10)] {
        let request = AddTwoIntsRequest { a, b };
        // `call()` rejects a new request while one is still in flight.
        // A timed-out `Promise` does NOT clear that flag on drop (a
        // stale reply could still be queued), so a call that times out
        // — e.g. the first one, before the request writer has finished
        // matching the server's reader — would otherwise wedge every
        // later call with `RequestInFlight`. Clear it explicitly on
        // failure so the next call proceeds, mirroring the C client.
        let got = match client.call(&request) {
            Ok(mut promise) => {
                match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
                    Ok(reply) => {
                        nros_info!(&LOGGER, "Response: {} + {} = {}", a, b, reply.sum);
                        true
                    }
                    Err(e) => {
                        nros_error!(&LOGGER, "Service call failed: {:?}", e);
                        false
                    }
                }
            }
            Err(e) => {
                nros_error!(&LOGGER, "Failed to send request: {:?}", e);
                false
            }
        };
        if got {
            ok += 1;
        } else {
            client.reset_in_flight();
        }
    }
    nros_info!(&LOGGER, "{}/4 calls succeeded", ok);

    if ok > 0 {
        0
    } else {
        1
    }
}
