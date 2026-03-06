//! Native Service Client Example
//!
//! Demonstrates a ROS 2 service client using nros with the Promise API.
//! The client sends a request (non-blocking), then waits for the reply
//! using `promise.wait()` which drives I/O internally.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Start the service server:
//! cargo run -p native-rs-service-server
//!
//! # Run the client:
//! cargo run -p native-rs-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros Service Client Example");
    info!("================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("add_two_ints_client");
    let mut executor: Executor<_> = Executor::open(&config).expect("Failed to open session");

    // Create node and service client
    let mut node = executor
        .create_node("add_two_ints_client")
        .expect("Failed to create node");
    info!("Node created: add_two_ints_client");

    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");
    info!("Service client created for: /add_two_ints");

    // Make several service calls using the Promise pattern
    let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let request = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);

        // Non-blocking: send request and get a promise
        let mut promise = match client.call(&request) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to send request: {:?}", e);
                std::process::exit(1);
            }
        };

        // Wait for the reply (drives I/O internally)
        let response = match promise.wait(&mut executor, 5000) {
            Ok(reply) => reply,
            Err(e) => {
                error!("Service call failed: {:?}", e);
                error!("Make sure the service server is running:");
                error!("  cargo run -p native-rs-service-server");
                std::process::exit(1);
            }
        };

        info!("Response: {} + {} = {}", a, b, response.sum);
        assert_eq!(response.sum, a + b, "Sum mismatch!");

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    info!("All service calls completed successfully!");
}
