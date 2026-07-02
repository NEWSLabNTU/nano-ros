//! Native RTIC-pattern Action Server
//!
//! Validates the RTIC action server pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `try_accept_goal()`, `publish_feedback()`, `complete_goal()`,
//!   `try_handle_get_result()` (all manual polling)
//!
//! This is the native equivalent of `examples/stm32f4/rust/rtic-action-server/`.

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::prelude::*;
use nros_log::{Logger, nros_error, nros_info};

// Diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-server-rtic");

extern crate nros_platform_cffi as _;

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros RTIC-pattern Action Server (native)");

    let config = ExecutorConfig::from_env().node_name("fibonacci_action_server");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_action_server")
        .expect("Failed to create node");
    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");

    nros_info!(&LOGGER, "Action server ready: /fibonacci");
    nros_info!(&LOGGER, "Waiting for action goals (RTIC pattern)...");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

    while std::time::Instant::now() < deadline {
        executor.spin_once(core::time::Duration::from_millis(0));

        // Try to accept new goals
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            nros_info!(&LOGGER, "Received goal request with order {}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;

                    server.set_goal_status(&goal_id, GoalStatus::Executing);
                    nros_info!(&LOGGER, "Executing goal");

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
                            nros_error!(&LOGGER, "Feedback error: {:?}", e);
                        } else {
                            nros_info!(&LOGGER, "Publish feedback");
                        }

                        // Drive I/O between feedback publishes
                        for _ in 0..10 {
                            executor.spin_once(core::time::Duration::from_millis(0));
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }

                    let result = FibonacciResult { sequence };
                    nros_info!(&LOGGER, "Goal succeeded");
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
            Err(e) => nros_error!(&LOGGER, "Accept error: {:?}", e),
        }

        // Handle cancel requests
        let _ = server.try_handle_cancel(|_id, _status| nros::CancelResponse::Ok);

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    nros_info!(&LOGGER, "Done");
}
