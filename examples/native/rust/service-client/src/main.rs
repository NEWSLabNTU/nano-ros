//! Native Service Client.
//!
//! Calls the `example_interfaces/srv/AddTwoInts` service once, matching the
//! official ROS 2 `demo_nodes_cpp` `add_two_ints_client` demo. The summands
//! come from the command line (`add_two_ints_client A B`), defaulting to
//! `2 3`. Single-file `[[bin]]`: explicit [`nros::init_with_launch_auto`]
//! then a user-owned request/wait sequence.
//!
//! ```bash
//! cargo run -p native-rs-service-server   # then this client
//! cargo run -p native-rs-service-client -- 2 3
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;

/// Service-client body — call `AddTwoInts` once and log the result.
fn run() -> i32 {
    // Summands from argv, defaulting to the official demo's `2 3`.
    let mut args = std::env::args().skip(1).filter_map(|s| s.parse().ok());
    let a: i64 = args.next().unwrap_or(2);
    let b: i64 = args.next().unwrap_or(3);

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_client");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let mut node = executor
        .create_node("add_two_ints_client")
        .expect("Failed to create node");
    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");

    if !matches!(
        client.wait_for_service(&mut executor, core::time::Duration::from_secs(5)),
        Ok(true)
    ) {
        error!("Timed out waiting for /add_two_ints service");
        return 1;
    }

    // One request, as in the official demo. Retry a timed-out call several
    // times: on Cyclone DDS the first request can race the request-writer
    // ↔ server endpoint match and get dropped, and a timed-out `Promise`
    // leaves the in-flight flag set (cleared via `reset_in_flight`).
    //
    // Issue 0153 / #180 — back off between attempts. On rmw_zenoh the server's
    // LIVELINESS token (what `wait_for_service` observes) gossips ahead of
    // its queryable ROUTE: a `z_get` fired in that window matches no
    // queryable and completes instantly with no reply, so tight retries all
    // burn out inside the same gossip gap. The gap is short (~1-3 s) against a
    // native peer but larger (~5-8 s) against a slow zephyr-pico embedded
    // server whose queryable declare lands well after its liveliness token
    // (#180). Retry for ~16 s total (8 × 2 s) so the window is spanned on
    // either; a native server still completes on attempt 0 with no delay.
    let request = AddTwoIntsRequest { a, b };
    for attempt in 0..8 {
        if attempt > 0 {
            std::thread::sleep(core::time::Duration::from_secs(2));
        }
        let mut promise = match client.call(&request) {
            Ok(promise) => promise,
            Err(e) => {
                error!("Failed to send request: {:?}", e);
                return 1;
            }
        };
        match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
            Ok(reply) => {
                info!("Result of add_two_ints: {}", reply.sum);
                return 0;
            }
            Err(e) => {
                error!("Service call failed (attempt {}): {:?}", attempt + 1, e);
                client.reset_in_flight();
            }
        }
    }
    1
}

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();
    std::process::exit(run());
}
