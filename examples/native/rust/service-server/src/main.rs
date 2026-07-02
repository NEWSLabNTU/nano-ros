//! Native Service Server.
//!
//! Serves `example_interfaces/srv/AddTwoInts` on `/add_two_ints`, matching
//! the official ROS 2 `demo_nodes_cpp` `add_two_ints_server` demo. Single-file
//! `[[bin]]`: explicit [`nros::init_with_launch_auto`] then a user-owned spin
//! loop.
//!
//! ```bash
//! cargo run -p native-rs-service-server   # then native-rs-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::prelude::*;

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_server");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            info!("Incoming request");
            info!("a: {} b: {}", request.a, request.b);
            AddTwoIntsResponse {
                sum: request.a + request.b,
            }
        })
        .expect("Failed to add service");
    info!("Waiting for service requests...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
