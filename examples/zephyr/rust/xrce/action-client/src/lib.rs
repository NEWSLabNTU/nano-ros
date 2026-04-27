//! nros Zephyr XRCE Action Client Example (Rust)
//!
//! A ROS 2 compatible action client running on Zephyr RTOS using the
//! XRCE-DDS backend. Uses the Promise API: `send_goal()` / `get_result()`
//! return promises that are resolved with `promise.wait()` which drives
//! I/O internally.

#![no_std]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use log::{error, info, warn};
use nros::{Executor, ExecutorConfig, NodeError};

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr XRCE Action Client");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    info!("Action: Fibonacci");

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    // XRCE locator: "agent_addr:port" (no tcp/ prefix).
    let config = ExecutorConfig::new("127.0.0.1:2018").node_name("xrce_action_client");
    let mut executor = Executor::open(&config)?;
    let mut node = executor.create_node("fibonacci_action_client")?;
    let mut action_client = node.create_action_client::<Fibonacci>("/fibonacci")?;

    info!("Action client ready: /fibonacci");

    info!("Waiting for server...");
    zephyr::time::sleep(zephyr::time::Duration::secs(3));

    let goal = FibonacciGoal { order: 10 };
    info!("Sending goal: order={}", goal.order);

    let (goal_id, mut promise) = action_client.send_goal(&goal)?;

    let accepted = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok(accepted) => accepted,
        Err(e) => {
            error!("Goal acceptance failed: {:?}", e);
            return Err(e);
        }
    };

    if !accepted {
        warn!("Goal was rejected by the server");
        return Ok(());
    }

    info!(
        "Goal accepted! ID: {:02x}{:02x}{:02x}{:02x}...",
        goal_id.uuid[0], goal_id.uuid[1], goal_id.uuid[2], goal_id.uuid[3]
    );

    info!("Waiting for feedback...");

    {
        let mut stream = action_client.feedback_stream_for(goal_id);
        let mut feedback_count: u32 = 0;
        for _ in 0..20 {
            match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
                Ok(Some(feedback)) => {
                    feedback_count += 1;
                    info!(
                        "Feedback #{}: {:?}",
                        feedback_count,
                        feedback.sequence.as_slice()
                    );

                    if feedback.sequence.len() as i32 > goal.order {
                        info!("Received all feedback, action completed!");
                        break;
                    }
                }
                Ok(None) => {
                    if feedback_count == 0 {
                        error!("No feedback received, retrying...");
                    }
                }
                Err(e) => {
                    error!("Feedback error: {:?}", e);
                    break;
                }
            }
        }
    }

    let mut result_promise = action_client.get_result(&goal_id)?;

    match result_promise.wait(&mut executor, core::time::Duration::from_millis(30000)) {
        Ok((status, result)) => {
            info!(
                "Result: status={:?}, sequence={:?}",
                status,
                result.sequence.as_slice()
            );
        }
        Err(e) => {
            error!("Failed to get result: {:?}", e);
        }
    }

    info!("Action client finished");

    loop {
        zephyr::time::sleep(zephyr::time::Duration::secs(10));
    }
}
