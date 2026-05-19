//! Native DDS Action Client Example
//!
//! ROS 2 Fibonacci action client using nros with the DDS/RTPS backend.
//! Brokerless peer-to-peer discovery — no router or agent.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p native-dds-action-server
//!
//! # In another terminal:
//! cargo run -p native-dds-action-client
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

    nros_info!(&LOGGER, "nros DDS Action Client Example");
    nros_info!(&LOGGER, "================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("fibonacci_action_client");
    // Phase 115.L.5 — install dust-dds C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_dds::register().expect("Failed to register RMW backend");
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

    // Allow time for SPDP/SEDP discovery on all 5 action channels
    // (send_goal/cancel_goal/get_result services + feedback/status pubs).
    // Without this, the immediate send_goal write happens before the
    // server's matching DataReader is discovered and is silently
    // dropped at the writer.
    std::thread::sleep(std::time::Duration::from_secs(3));

    let order = std::env::var("NROS_FIBONACCI_ORDER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let cancel_after_feedback = std::env::var("NROS_ACTION_CANCEL_AFTER_FEEDBACK")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());

    // Create goal
    let goal = FibonacciGoal { order };
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
    let mut feedback_count = 0;
    let mut should_cancel = false;
    {
        let mut stream = client.feedback_stream_for(goal_id);
        for _ in 0..30 {
            // 30 x 1000ms = 30 second max
            match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
                Ok(Some(feedback)) => {
                    feedback_count += 1;
                    nros_info!(&LOGGER, "Feedback #{}: {:?}", feedback_count, feedback.sequence);

                    if let Some(cancel_after) = cancel_after_feedback
                        && feedback_count >= cancel_after
                    {
                        should_cancel = true;
                        break;
                    }

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
    }

    if should_cancel {
        nros_info!(&LOGGER, 
            "Requesting cancellation after {} feedback frames",
            feedback_count
        );
        let mut cancel_promise = match client.cancel_goal(&goal_id) {
            Ok(promise) => promise,
            Err(e) => {
                nros_error!(&LOGGER, "Failed to request cancellation: {:?}", e);
                std::process::exit(1);
            }
        };

        match cancel_promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
            Ok(response) => nros_info!(&LOGGER, "Cancel response: {:?}", response),
            Err(e) => {
                nros_error!(&LOGGER, "Cancel response failed: {:?}", e);
                std::process::exit(1);
            }
        }
    }

    // Give the server a few spins to store the terminal result before the
    // explicit get_result request.
    for _ in 0..5 {
        executor.spin_once(core::time::Duration::from_millis(100));
    }

    let mut result_promise = match client.get_result(&goal_id) {
        Ok(promise) => promise,
        Err(e) => {
            nros_error!(&LOGGER, "Failed to request result: {:?}", e);
            std::process::exit(1);
        }
    };

    let (status, result) =
        match result_promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
            Ok(result) => result,
            Err(e) => {
                nros_error!(&LOGGER, "Result response failed: {:?}", e);
                std::process::exit(1);
            }
        };

    nros_info!(&LOGGER, 
        "Result: status={:?}, sequence={:?}",
        status, result.sequence
    );

    if should_cancel {
        if status != GoalStatus::Canceled {
            nros_error!(&LOGGER, "Expected canceled result, got {:?}", status);
            std::process::exit(1);
        }
    } else if status != GoalStatus::Succeeded {
        nros_error!(&LOGGER, "Expected succeeded result, got {:?}", status);
        std::process::exit(1);
    }

    nros_info!(&LOGGER, "Action client finished");
}
