//! Native DDS Service Server Example
//!
//! ROS 2 service server using nros with the DDS/RTPS backend.
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

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros DDS Service Server Example");
    info!("================================");

    let config = ExecutorConfig::from_env().node_name("add_two_ints_server");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open DDS session");

    info!("Node created: add_two_ints_server");

    executor
        .add_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            info!("Received request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    info!("Service server created: /add_two_ints");

    info!("Waiting for service requests...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
