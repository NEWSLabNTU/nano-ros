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

#[cfg(not(feature = "zenoh"))]
use log::info;
#[cfg(feature = "zenoh")]
use log::{error, info};

#[cfg(feature = "zenoh")]
use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
#[cfg(feature = "zenoh")]
use nros::prelude::*;

#[cfg(feature = "zenoh")]
fn main() {
    env_logger::init();

    info!("nros Service Server Example");
    info!("================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");

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

#[cfg(not(feature = "zenoh"))]
fn main() {
    env_logger::init();
    info!("nros Service Server Example");
    info!("================================");
    info!("This example requires the 'zenoh' feature.");
    info!("Run with: cargo run -p native-rs-service-server --features zenoh");
}
