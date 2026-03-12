//! Native Action Server Example
//!
//! Demonstrates a ROS 2 action server using nros with the Executor API.
//! This example implements a Fibonacci action that computes the Fibonacci
//! sequence up to a given order, sending feedback as it computes.
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

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use log::{error, info};
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros Action Server Example");
    info!("================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("fibonacci_action_server");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    // Create node and action server
    let mut node = executor
        .create_node("fibonacci_action_server")
        .expect("Failed to create node");
    info!("Node created: fibonacci_action_server");

    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");
    info!("Action server created: /fibonacci");

    info!("Waiting for action goals...");
    info!("(Run native-rs-action-client in another terminal)");

    // Main loop - handle incoming goals
    loop {
        // Try to accept new goals
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            info!("Received goal request: order={}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                info!("Goal accepted: {}", goal_id);

                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;

                    server.set_goal_status(&goal_id, GoalStatus::Executing);

                    // Compute Fibonacci sequence with feedback
                    let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();

                    for i in 0..=order {
                        let next_val = if i == 0 {
                            0
                        } else if i == 1 {
                            1
                        } else {
                            let len = sequence.len();
                            sequence[len - 1] + sequence[len - 2]
                        };

                        let _ = sequence.push(next_val);

                        let feedback = FibonacciFeedback {
                            sequence: sequence.clone(),
                        };

                        if let Err(e) = server.publish_feedback(&goal_id, &feedback) {
                            error!("Failed to publish feedback: {:?}", e);
                        } else {
                            info!("Feedback: {:?}", feedback.sequence);
                        }

                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }

                    let result = FibonacciResult { sequence };
                    info!("Goal completed: {:?}", result.sequence);

                    server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                }
            }
            Ok(None) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                error!("Error accepting goal: {:?}", e);
            }
        }
    }
}
