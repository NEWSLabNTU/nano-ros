//! Native Service Server Example
//!
//! Demonstrates a ROS 2 service server using nros with the Executor API.
//! Uses callback-based service handling via spin_blocking().
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Run the service server:
//! cargo run -p native-rs-service-server
//!
//! # In another terminal, run the client:
//! cargo run -p native-rs-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros Service Server Example");
    info!("================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    info!("Node created: add_two_ints_server");

    // Register service callback
    executor
        .add_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            info!("Received request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    info!("Service server created: /add_two_ints");

    info!("Waiting for service requests...");
    info!("(Run native-rs-service-client in another terminal)");

    // Blocking spin loop
    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
