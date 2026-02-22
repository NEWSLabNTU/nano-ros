//! Native Action Client Example
//!
//! Demonstrates a ROS 2 action client using nros with the Promise API.
//! Sends a Fibonacci goal, waits for acceptance with `promise.wait()`,
//! then polls for feedback as the sequence is computed.
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
use log::{error, info, warn};
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros Action Client Example");
    info!("================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("fibonacci_action_client");
    let mut executor = Executor::<_, 8, 8192>::open(&config).expect("Failed to open session");

    // Create node and action client
    let mut node = executor
        .create_node("fibonacci_action_client")
        .expect("Failed to create node");
    info!("Node created: fibonacci_action_client");

    let mut client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");
    info!("Action client created: /fibonacci");

    // Create goal
    let goal = FibonacciGoal { order: 10 };
    info!("Sending goal: order={}", goal.order);

    // Send goal using the Promise pattern
    let (goal_id, mut promise) = match client.send_goal(&goal) {
        Ok(pair) => pair,
        Err(e) => {
            error!("Failed to send goal: {:?}", e);
            std::process::exit(1);
        }
    };

    // Wait for goal acceptance (drives I/O internally)
    let accepted = match promise.wait(&mut executor, 10000) {
        Ok(accepted) => accepted,
        Err(e) => {
            error!("Goal acceptance failed: {:?}", e);
            std::process::exit(1);
        }
    };

    if !accepted {
        warn!("Goal was rejected by the server");
        std::process::exit(1);
    }
    info!("Goal accepted! ID: {:?}", goal_id);

    info!("Waiting for feedback and result...");

    // Poll for feedback
    let mut feedback_count = 0;
    let start_time = std::time::Instant::now();
    let feedback_timeout = std::time::Duration::from_secs(30);

    loop {
        if start_time.elapsed() > feedback_timeout {
            error!("Timeout waiting for result");
            break;
        }

        executor.spin_once(100);

        match client.try_recv_feedback() {
            Ok(Some((fid, feedback))) => {
                if fid == goal_id {
                    feedback_count += 1;
                    info!("Feedback #{}: {:?}", feedback_count, feedback.sequence);

                    if feedback.sequence.len() as i32 > goal.order {
                        info!("Received all feedback, action completed!");
                        info!("Final sequence: {:?}", feedback.sequence);
                        break;
                    }
                }
            }
            Ok(None) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                error!("Error receiving feedback: {:?}", e);
            }
        }
    }

    info!("Action client finished");
}
