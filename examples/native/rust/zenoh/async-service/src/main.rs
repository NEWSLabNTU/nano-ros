//! Async Service Client Example
//!
//! Demonstrates the async/await API for service calls using nros.
//! Uses `embassy_futures::select` to concurrently drive I/O (`spin_async`)
//! and await a service reply (`Promise` as a `Future`).
//!
//! This pattern allows subscription callbacks, timers, and other handlers
//! to fire while a service call is in flight — solving the fundamental
//! problem of blocking `call()` on single-threaded embedded systems.
//!
//! # Usage
//!
//! ```bash
//! # Terminal 1: Start zenoh router
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Terminal 2: Start the service server
//! cargo run -p native-rs-service-server
//!
//! # Terminal 3: Run this async client
//! cargo run -p native-rs-async-service
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// Count of subscription messages received while service calls are in flight.
static SUB_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    env_logger::init();

    info!("nros Async Service Client Example");
    info!("==================================");

    // Create executor with enough capacity for subscription + service client
    let config = ExecutorConfig::from_env().node_name("async_service_client");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");

    // Register a subscription to show callbacks fire during async service calls
    executor
        .add_subscription::<example_interfaces::srv::AddTwoIntsRequest, _>(
            "/async_demo_heartbeat",
            |_msg: &AddTwoIntsRequest| {
                SUB_COUNT.fetch_add(1, Ordering::Relaxed);
            },
        )
        .expect("Failed to add subscription");

    // Create node and service client
    let mut node = executor
        .create_node("async_service_client")
        .expect("Failed to create node");

    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create client");

    info!("Service client created for: /add_two_ints");
    info!("Using async/await pattern with embassy_futures::select");

    // Run the async workflow with block_on
    nros::block_on(async {
        let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

        for (a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            info!("Calling service: {} + {} = ?", a, b);

            // Send the request (non-blocking), get a Promise future
            let promise = match client.call(&request) {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to send request: {:?}", e);
                    std::process::exit(1);
                }
            };

            // Use select to concurrently:
            //   - spin_async(): drives I/O so the reply can arrive
            //   - promise: resolves when the reply is received
            let response =
                match embassy_futures::select::select(executor.spin_async(), promise).await {
                    // spin_async() returns `!` so this branch is unreachable
                    embassy_futures::select::Either::First(never) => match never {},
                    embassy_futures::select::Either::Second(result) => match result {
                        Ok(reply) => reply,
                        Err(e) => {
                            error!("Service call failed: {:?}", e);
                            std::process::exit(1);
                        }
                    },
                };

            info!("Response: {} + {} = {}", a, b, response.sum);
            assert_eq!(response.sum, a + b, "Sum mismatch!");

            // Brief pause between calls
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        let sub_msgs = SUB_COUNT.load(Ordering::Relaxed);
        if sub_msgs > 0 {
            info!(
                "Subscription received {} messages during service calls",
                sub_msgs
            );
        }

        info!("All async service calls completed successfully!");
    });
}
