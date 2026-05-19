//! Native DDS Action Server Example
//!
//! ROS 2 Fibonacci action server using nros with the DDS/RTPS backend.
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

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};
use nros::{CancelResponse, prelude::*};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-server");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros DDS Action Server Example");
    nros_info!(&LOGGER, "================================");

    // Create executor from environment
    let config = ExecutorConfig::from_env().node_name("fibonacci_action_server");
    // Phase 115.L.5 — install dust-dds C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_dds::register().expect("Failed to register RMW backend");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    // Create node and action server
    let mut node = executor
        .create_node("fibonacci_action_server")
        .expect("Failed to create node");
    nros_info!(&LOGGER, "Node created: fibonacci_action_server");

    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");
    nros_info!(&LOGGER, "Action server created: /fibonacci");

    nros_info!(&LOGGER, "Waiting for action goals...");
    nros_info!(&LOGGER, "(Run native-rs-action-client in another terminal)");

    // Main loop - handle incoming goals
    loop {
        executor.spin_once(core::time::Duration::from_millis(20));

        if let Err(e) = server.try_handle_get_result() {
            nros_error!(&LOGGER, "Error handling result request: {:?}", e);
        }

        if let Err(e) = server.try_handle_cancel(|goal_id, status| {
            nros_info!(&LOGGER, "Cancel request: {} status={:?}", goal_id, status);
            if status.is_active() {
                CancelResponse::Ok
            } else {
                CancelResponse::GoalTerminated
            }
        }) {
            nros_error!(&LOGGER, "Error handling cancel request: {:?}", e);
        }

        // Try to accept new goals
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            nros_info!(&LOGGER, "Received goal request: order={}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                nros_info!(&LOGGER, "Goal accepted: {}", goal_id);

                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;

                    server.set_goal_status(&goal_id, GoalStatus::Executing);

                    // Compute Fibonacci sequence with feedback
                    let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();
                    let mut canceled = false;

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
                            nros_error!(&LOGGER, "Failed to publish feedback: {:?}", e);
                        } else {
                            nros_info!(&LOGGER, "Feedback: {:?}", feedback.sequence);
                        }

                        executor.spin_once(core::time::Duration::from_millis(20));

                        match server.try_handle_cancel(|cancel_id, status| {
                            nros_info!(&LOGGER, "Cancel request: {} status={:?}", cancel_id, status);
                            if cancel_id.uuid == goal_id.uuid && status.is_active() {
                                CancelResponse::Ok
                            } else if status.is_terminal() {
                                CancelResponse::GoalTerminated
                            } else {
                                CancelResponse::UnknownGoal
                            }
                        }) {
                            Ok(Some((cancel_id, CancelResponse::Ok)))
                                if cancel_id.uuid == goal_id.uuid =>
                            {
                                nros_info!(&LOGGER, "Goal cancellation accepted: {}", goal_id);
                                canceled = true;
                            }
                            Ok(_) => {}
                            Err(e) => nros_error!(&LOGGER, "Error handling cancel request: {:?}", e),
                        }

                        if let Err(e) = server.try_handle_get_result() {
                            nros_error!(&LOGGER, "Error handling result request: {:?}", e);
                        }

                        if canceled {
                            break;
                        }

                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }

                    let result = FibonacciResult { sequence };
                    if canceled {
                        nros_info!(&LOGGER, "Goal canceled: {:?}", result.sequence);
                        server.complete_goal(&goal_id, GoalStatus::Canceled, result);
                    } else {
                        nros_info!(&LOGGER, "Goal completed: {:?}", result.sequence);
                        server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                    }
                }
            }
            Ok(None) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                nros_error!(&LOGGER, "Error accepting goal: {:?}", e);
            }
        }
    }
}
