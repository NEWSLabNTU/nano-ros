//! nros Zephyr DDS Action Server Example (Rust)
//!
//! ROS 2 / DDS-RTPS action server running on Zephyr RTOS via dust-dds.
//! Implements the Fibonacci action — computing Fibonacci sequences
//! with progress feedback.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};
use log::{error, info};
use nros::{
    CancelResponse, Executor, ExecutorConfig, GoalResponse, GoalStatus, NodeError,
};

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr DDS Action Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    info!("Action: Fibonacci");

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);

    let config = ExecutorConfig::new("")
        .domain_id(0)
        .node_name("dds_action_server");
    let mut executor = Executor::open(&config)?;
    let mut node = executor.create_node("fibonacci_action_server")?;
    let mut action_server = node.create_action_server::<Fibonacci>("/fibonacci")?;

    info!("Action server ready: /fibonacci");
    info!("Waiting for action goals...");

    loop {
        executor.spin_once(core::time::Duration::from_millis(100));

        let _ = action_server.try_handle_cancel(|_goal_id, status| {
            if status == GoalStatus::Executing || status == GoalStatus::Accepted {
                info!("Goal cancellation accepted");
                CancelResponse::Ok
            } else {
                CancelResponse::GoalTerminated
            }
        });

        let _ = action_server.try_handle_get_result();

        let accepted = action_server.try_accept_goal(|_goal_id, goal| {
            info!("Goal request: order={}", goal.order);
            if goal.order >= 0 {
                GoalResponse::AcceptAndExecute
            } else {
                GoalResponse::Reject
            }
        })?;

        if let Some(goal_id) = accepted {
            let order = match action_server.get_goal(&goal_id) {
                Some(g) => g.goal.order,
                None => continue,
            };

            info!("Executing goal: order={}", order);
            action_server.set_goal_status(&goal_id, GoalStatus::Executing);

            let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();
            let mut cancelled = false;

            for i in 0..=order {
                executor.spin_once(core::time::Duration::from_millis(10));
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

                let feedback = FibonacciFeedback {
                    sequence: sequence.clone(),
                };
                if let Err(e) = action_server.publish_feedback(&goal_id, &feedback) {
                    error!("Failed to publish feedback: {:?}", e);
                } else {
                    info!("Feedback: {:?}", feedback.sequence);
                }

                zephyr::time::sleep(zephyr::time::Duration::millis(500));
            }

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
