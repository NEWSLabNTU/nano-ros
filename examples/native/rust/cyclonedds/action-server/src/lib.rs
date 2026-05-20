//! Native Rust action server over the Cyclone DDS RMW backend.
//!
//! Phase 171.0.b — native rust cyclonedds is cmake-driven: this crate
//! compiles to a `staticlib` named `rustapp` exposing a C `rust_main()`
//! entry. The per-example `CMakeLists.txt` runs
//! `nros_generate_interfaces(...)` for example_interfaces + action_msgs
//! + unique_identifier_msgs + builtin_interfaces (the action's eight
//! wrapper descriptors plus the cancel/status infrastructure types),
//! builds the C++ `nros-rmw-cyclonedds` backend, and links both
//! alongside this staticlib + `libddsc` + `stdc++`.

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::prelude::*;
use nros_log::{nros_error, nros_info, Logger};

static LOGGER: Logger = Logger::new("action-server");

extern crate nros_platform_cffi as _;

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    if nros_rmw_cyclonedds_sys::register().is_err() {
        nros_error!(&LOGGER, "Failed to register Cyclone DDS RMW backend");
        return 1;
    }
    nros_info!(&LOGGER, "nros Native Action Server (Cyclone DDS Transport)");

    let config = ExecutorConfig::from_env().node_name("fibonacci_action_server");
    let mut executor: Executor = match Executor::open(&config) {
        Ok(e) => e,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to open executor");
            return 1;
        }
    };

    let mut node = match executor.create_node("fibonacci_action_server") {
        Ok(n) => n,
        Err(_) => return 1,
    };
    let mut server = match node.create_action_server::<Fibonacci>("/fibonacci") {
        Ok(s) => s,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to create action server");
            return 1;
        }
    };
    nros_info!(&LOGGER, "Action server created: /fibonacci");
    nros_info!(&LOGGER, "Waiting for action goals...");

    loop {
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            nros_info!(&LOGGER, "Received goal request: order={}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                nros_info!(&LOGGER, "Goal accepted: {}", goal_id);
                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;
                    server.set_goal_status(&goal_id, GoalStatus::Executing);

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
                        if server.publish_feedback(&goal_id, &feedback).is_ok() {
                            nros_info!(&LOGGER, "Feedback: {:?}", feedback.sequence);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }

                    let result = FibonacciResult { sequence };
                    nros_info!(&LOGGER, "Goal completed: {:?}", result.sequence);
                    server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                }
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => nros_error!(&LOGGER, "Error accepting goal: {:?}", e),
        }
    }
}
