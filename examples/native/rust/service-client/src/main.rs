//! Native Service Client — Phase 212.L.2 Application pkg shape.
//!
//! Calls the `example_interfaces/srv/AddTwoInts` service a handful of
//! times. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] (Pattern 2) then a user-owned
//! request/wait loop.
//!
//! ```bash
//! cargo run -p native-rs-service-server   # then this client
//! cargo run -p native-rs-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!("this example requires exactly one of `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce`",);

// Phase 227.3 (unified RMW) — backend self-registers via nros's __FORCE_LINK_* + the cffi walker; no register() call.

/// Service-client body — call `AddTwoInts` a few times. A failed
/// `call()` clears the in-flight flag (`reset_in_flight`) and continues
/// rather than aborting: on Cyclone DDS the first call can race the
/// request-writer ↔ server endpoint match and time out, and a timed-out
/// `Promise` does not clear that flag on drop (later calls would
/// otherwise wedge with `RequestInFlight`). Zenoh's discovery is fast
/// enough that all calls succeed.
fn run() -> i32 {
    info!("nros Service Client Example");
    info!("================================");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_client");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let mut node = executor
        .create_node("add_two_ints_client")
        .expect("Failed to create node");
    info!("Node created: add_two_ints_client");
    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");
    info!("Service client created for: /add_two_ints");

    match client.wait_for_service(&mut executor, core::time::Duration::from_secs(5)) {
        Ok(true) => info!("Service server is available"),
        _ => {
            error!("Timed out waiting for /add_two_ints service");
            return 1;
        }
    }

    let mut ok = 0;
    for (a, b) in [(5, 3), (10, 20), (100, 200), (-5, 10)] {
        let request = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);
        let got = match client.call(&request) {
            Ok(mut promise) => {
                match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
                    Ok(reply) => {
                        info!("Response: {} + {} = {}", a, b, reply.sum);
                        true
                    }
                    Err(e) => {
                        error!("Service call failed: {:?}", e);
                        false
                    }
                }
            }
            Err(e) => {
                error!("Failed to send request: {:?}", e);
                false
            }
        };
        if got {
            ok += 1;
        } else {
            client.reset_in_flight();
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    info!("{}/4 service calls succeeded", ok);
    if ok > 0 { 0 } else { 1 }
}

fn main() {
    env_logger::init();
    std::process::exit(run());
}
