//! Native RTIC-pattern Action Server
//!
//! Validates the RTIC action server pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `try_accept_goal()`, `publish_feedback()`, `complete_goal()`,
//!   `try_handle_get_result()` (all manual polling)
//!
//! This is the native equivalent of `examples/stm32f4/rust/zenoh/rtic-action-server/`.

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use log::info;
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros RTIC-pattern Action Server (native)");

    let config = ExecutorConfig::from_env().node_name("fibonacci_server");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_server")
        .expect("Failed to create node");
    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");

    info!("Action server ready: /fibonacci");
    info!("Waiting for goals (RTIC pattern)...");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

    while std::time::Instant::now() < deadline {
        executor.spin_once(core::time::Duration::from_millis(0));

        // Try to accept new goals
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            info!("Goal request: order={}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                info!("Goal accepted: {}", goal_id);

                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;

                    server.set_goal_status(&goal_id, GoalStatus::Executing);

                    // Compute Fibonacci with feedback
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
                            log::error!("Feedback error: {:?}", e);
                        } else {
                            info!("Feedback: {:?}", &feedback.sequence[..]);
                        }

                        // Drive I/O between feedback publishes
                        for _ in 0..10 {
                            executor.spin_once(core::time::Duration::from_millis(0));
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }

                    let result = FibonacciResult { sequence };
                    info!("Goal completed: {:?}", &result.sequence[..]);
                    server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                }

                // Handle get_result requests after completing
                for _ in 0..200 {
                    let _ = server.try_handle_get_result();
                    executor.spin_once(core::time::Duration::from_millis(0));
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            Ok(None) => {}
            Err(e) => log::error!("Accept error: {:?}", e),
        }

        // Handle cancel requests
        let _ = server.try_handle_cancel(|_id, _status| nros::CancelResponse::Ok);

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    info!("Done");
}
