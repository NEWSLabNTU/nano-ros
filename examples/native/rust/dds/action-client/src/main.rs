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
use log::{error, info, warn};
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros DDS Action Client Example");
    info!("================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("fibonacci_action_client");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    // Create node and action client
    let mut node = executor
        .create_node("fibonacci_action_client")
        .expect("Failed to create node");
    info!("Node created: fibonacci_action_client");

    let mut client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");
    info!("Action client created: /fibonacci");

    // Allow time for SPDP/SEDP discovery on all 5 action channels
    // (send_goal/cancel_goal/get_result services + feedback/status pubs).
    // Without this, the immediate send_goal write happens before the
    // server's matching DataReader is discovered and is silently
    // dropped at the writer.
    std::thread::sleep(std::time::Duration::from_secs(3));

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
    let accepted = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
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

    info!("Waiting for feedback...");

    // Receive feedback via FeedbackStream (drives I/O internally, filters by goal ID)
    let mut stream = client.feedback_stream_for(goal_id);
    let mut feedback_count = 0;
    for _ in 0..30 {
        // 30 x 1000ms = 30 second max
        match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
            Ok(Some(feedback)) => {
                feedback_count += 1;
                info!("Feedback #{}: {:?}", feedback_count, feedback.sequence);

                if feedback.sequence.len() as i32 > goal.order {
                    info!("Received all feedback, action completed!");
                    info!("Final sequence: {:?}", feedback.sequence);
                    break;
                }
            }
            Ok(None) => {} // no feedback in this window, retry
            Err(e) => {
                error!("Error receiving feedback: {:?}", e);
                break;
            }
        }
    }

    info!("Action client finished");
}
