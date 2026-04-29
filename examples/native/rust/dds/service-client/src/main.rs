//! Native DDS Service Client Example
//!
//! ROS 2 service client using nros with the DDS/RTPS backend.
//! Uses brokerless peer-to-peer discovery — no router or agent needed.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p native-dds-service-server
//!
//! # In another terminal:
//! cargo run -p native-dds-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros DDS Service Client Example");
    info!("================================");

    let config = ExecutorConfig::from_env().node_name("add_two_ints_client");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    let mut node = executor
        .create_node("add_two_ints_client")
        .expect("Failed to create node");
    info!("Node created: add_two_ints_client");

    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");
    info!("Service client created for: /add_two_ints");

    // Allow time for SPDP/SEDP discovery to complete (DDS service
    // discovery is slower than zenoh's broker-mediated routing).
    std::thread::sleep(std::time::Duration::from_secs(3));

    let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let request = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);

        let mut promise = match client.call(&request) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to send request: {:?}", e);
                std::process::exit(1);
            }
        };

        let response = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
            Ok(reply) => reply,
            Err(e) => {
                error!("Service call failed: {:?}", e);
                error!("Make sure the service server is running:");
                error!("  cargo run -p native-dds-service-server");
                std::process::exit(1);
            }
        };

        info!("Response: {} + {} = {}", a, b, response.sum);
        assert_eq!(response.sum, a + b, "Sum mismatch!");

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    info!("All service calls completed successfully!");
}
