//! Native Action Client Example
//!
//! Demonstrates a ROS 2 action client using nros with the Promise API.
//! Sends a Fibonacci goal, waits for acceptance with `promise.wait()`,
//! then receives feedback via `FeedbackStream::wait_next()`.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Run the action server:
//! cargo run -p native-rs-action-server
//!
//! # In another terminal, run the client:
//! cargo run -p native-rs-action-client
//! ```

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};
use nros::prelude::*;

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-client");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros Action Client Example");
    nros_info!(&LOGGER, "================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("fibonacci_action_client");
    // Phase 115.L.5 — install zenoh-pico C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    // Create node and action client
    let mut node = executor
        .create_node("fibonacci_action_client")
        .expect("Failed to create node");
    nros_info!(&LOGGER, "Node created: fibonacci_action_client");

    let mut client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");
    nros_info!(&LOGGER, "Action client created: /fibonacci");

    // Create goal
    let goal = FibonacciGoal { order: 10 };
    nros_info!(&LOGGER, "Sending goal: order={}", goal.order);

    // Send goal using the Promise pattern
    let (goal_id, mut promise) = match client.send_goal(&goal) {
        Ok(pair) => pair,
        Err(e) => {
            nros_error!(&LOGGER, "Failed to send goal: {:?}", e);
            std::process::exit(1);
        }
    };

    // Wait for goal acceptance (drives I/O internally)
    let accepted = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok(accepted) => accepted,
        Err(e) => {
            nros_error!(&LOGGER, "Goal acceptance failed: {:?}", e);
            std::process::exit(1);
        }
    };

    if !accepted {
        nros_warn!(&LOGGER, "Goal was rejected by the server");
        std::process::exit(1);
    }
    nros_info!(&LOGGER, "Goal accepted! ID: {:?}", goal_id);

    nros_info!(&LOGGER, "Waiting for feedback...");

    // Receive feedback via FeedbackStream (drives I/O internally, filters by goal ID)
    let mut stream = client.feedback_stream_for(goal_id);
    let mut feedback_count = 0;
    for _ in 0..30 {
        // 30 x 1000ms = 30 second max
        match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
            Ok(Some(feedback)) => {
                feedback_count += 1;
                nros_info!(&LOGGER, "Feedback #{}: {:?}", feedback_count, feedback.sequence);

                if feedback.sequence.len() as i32 > goal.order {
                    nros_info!(&LOGGER, "Received all feedback, action completed!");
                    nros_info!(&LOGGER, "Final sequence: {:?}", feedback.sequence);
                    break;
                }
            }
            Ok(None) => {} // no feedback in this window, retry
            Err(e) => {
                nros_error!(&LOGGER, "Error receiving feedback: {:?}", e);
                break;
            }
        }
    }

    nros_info!(&LOGGER, "Action client finished");
}
