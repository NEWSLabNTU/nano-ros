//! nros Zephyr Action Server Example (Rust)
//!
//! A ROS 2 compatible action server running on Zephyr RTOS using the nros API.
//! The server implements the Fibonacci action - computing Fibonacci sequences
//! with progress feedback.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};
use log::{error, info};
use nros::{CancelResponse, GoalResponse, GoalStatus, ShimExecutor, ShimNodeError};

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Action Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    info!("Action: Fibonacci");

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), ShimNodeError> {
    let mut executor = ShimExecutor::new(b"tcp/192.0.2.2:7447\0")?;
    let mut node = executor.create_node("fibonacci_action_server")?;
    let mut action_server = node.create_action_server::<Fibonacci>("/fibonacci")?;

    info!("Action server ready: /fibonacci");
    info!("Waiting for action goals...");

    loop {
        let _ = executor.spin_once(100);

        // Handle cancel requests
        let _ = action_server.try_handle_cancel(|_goal_id, status| {
            if status == GoalStatus::Executing || status == GoalStatus::Accepted {
                info!("Goal cancellation accepted");
                CancelResponse::Ok
            } else {
                CancelResponse::GoalTerminated
            }
        });

        // Handle get_result requests
        let _ = action_server.try_handle_get_result();

        // Try to accept a new goal
        let accepted = action_server.try_accept_goal(|goal| {
            info!("Goal request: order={}", goal.order);
            if goal.order >= 0 {
                GoalResponse::AcceptAndExecute
            } else {
                GoalResponse::Reject
            }
        })?;

        if let Some(goal_id) = accepted {
            // Get the goal data
            let order = match action_server.get_goal(&goal_id) {
                Some(g) => g.goal.order,
                None => continue,
            };

            info!("Executing goal: order={}", order);
            action_server.set_goal_status(&goal_id, GoalStatus::Executing);

            // Compute Fibonacci sequence with feedback
            let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();
            let mut cancelled = false;

            for i in 0..=order {
                // Process events (including cancel requests)
                let _ = executor.spin_once(10);
                let _ = action_server.try_handle_cancel(|cid, status| {
                    if cid.uuid == goal_id.uuid
                        && (status == GoalStatus::Executing || status == GoalStatus::Accepted)
                    {
                        CancelResponse::Ok
                    } else if status == GoalStatus::Executing || status == GoalStatus::Accepted {
                        CancelResponse::Ok
                    } else {
                        CancelResponse::GoalTerminated
                    }
                });

                // Check for cancellation
                if let Some(g) = action_server.get_goal(&goal_id) {
                    if g.status == GoalStatus::Canceling {
                        info!("Goal cancelled at step {}", i);
                        cancelled = true;
                        break;
                    }
                }

                let next_val = if i == 0 {
                    0
                } else if i == 1 {
                    1
                } else {
                    let len = sequence.len();
                    sequence[len - 1] + sequence[len - 2]
                };
                let _ = sequence.push(next_val);

                // Send feedback
                let feedback = FibonacciFeedback {
                    sequence: sequence.clone(),
                };
                if let Err(e) = action_server.publish_feedback(&goal_id, &feedback) {
                    error!("Failed to publish feedback: {:?}", e);
                } else {
                    info!("Feedback: {:?}", feedback.sequence);
                }

                // Simulate computation time
                zephyr::time::sleep(zephyr::time::Duration::millis(500));
            }

            // Complete the goal
            let result = FibonacciResult { sequence };
            if cancelled {
                info!("Goal canceled");
                action_server.complete_goal(&goal_id, GoalStatus::Canceled, result);
            } else {
                info!("Goal succeeded");
                action_server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
            }
        }
    }
}
