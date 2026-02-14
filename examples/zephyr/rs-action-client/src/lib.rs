//! nros Zephyr Action Client Example (Rust)
//!
//! A ROS 2 compatible action client running on Zephyr RTOS using the nros API.
//! The client sends a Fibonacci goal and receives feedback as the sequence
//! is computed.

#![no_std]

use log::{error, info, warn};
use nros::{ShimExecutor, ShimNodeError};
use example_interfaces::action::{Fibonacci, FibonacciGoal};

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Action Client");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    info!("Action: Fibonacci");

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), ShimNodeError> {
    let mut executor = ShimExecutor::new(b"tcp/192.0.2.2:7447\0")?;
    let mut node = executor.create_node("fibonacci_action_client")?;
    let mut action_client = node.create_action_client::<Fibonacci>("/fibonacci")?;

    info!("Action client ready: /fibonacci");

    // Allow time for connection to stabilize
    info!("Waiting for server...");
    zephyr::time::sleep(zephyr::time::Duration::secs(3));

    // Send goal
    let goal = FibonacciGoal { order: 10 };
    info!("Sending goal: order={}", goal.order);

    let goal_id = match action_client.send_goal(&goal) {
        Ok(id) => {
            info!(
                "Goal accepted! ID: {:02x}{:02x}{:02x}{:02x}...",
                id.uuid[0], id.uuid[1], id.uuid[2], id.uuid[3]
            );
            id
        }
        Err(ShimNodeError::ServiceRequestFailed) => {
            warn!("Goal was rejected by the server");
            return Ok(());
        }
        Err(e) => {
            error!("Failed to send goal: {:?}", e);
            return Err(e);
        }
    };

    info!("Waiting for feedback and result...");

    // Wait for feedback
    let mut feedback_count: u32 = 0;
    let mut no_feedback_cycles: u32 = 0;
    let max_wait_cycles = 200; // 20 seconds max (100ms per cycle)

    for cycle in 0..max_wait_cycles {
        let _ = executor.spin_once(100);

        // Check for feedback
        match action_client.try_recv_feedback() {
            Ok(Some((fid, feedback))) => {
                if fid.uuid == goal_id.uuid {
                    feedback_count += 1;
                    info!("Feedback #{}: {:?}", feedback_count, feedback.sequence.as_slice());
                    no_feedback_cycles = 0;

                    // Check if we have all feedback (order + 1 values)
                    if feedback.sequence.len() as i32 > goal.order {
                        info!("Received all feedback, action completed!");
                        break;
                    }
                }
            }
            Ok(None) => {
                no_feedback_cycles += 1;
                if no_feedback_cycles > 50 && feedback_count == 0 {
                    error!("No feedback received after 5 seconds");
                    break;
                }
            }
            Err(e) => {
                error!("Feedback error: {:?}", e);
            }
        }

        if cycle == max_wait_cycles - 1 {
            error!("Timeout waiting for action completion");
        }
    }

    // Get final result
    match action_client.get_result(&goal_id) {
        Ok((status, result)) => {
            info!("Result: status={:?}, sequence={:?}", status, result.sequence.as_slice());
        }
        Err(e) => {
            error!("Failed to get result: {:?}", e);
        }
    }

    info!("Action client finished");

    // Keep alive
    loop {
        zephyr::time::sleep(zephyr::time::Duration::secs(10));
    }
}
