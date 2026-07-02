//! Native Action Server.
//!
//! Serves the `example_interfaces/action/Fibonacci` action on `/fibonacci`,
//! matching the official ROS 2 `action_tutorials` `fibonacci_action_server`
//! demo: accept a goal, publish the growing sequence as feedback, then
//! succeed with the full sequence. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] then a user-owned spin loop.
//!
//! ```bash
//! cargo run -p native-rs-action-server   # then native-rs-action-client
//! ```

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use log::{error, info};
use nros::prelude::*;

fn main() -> ! {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();

    // The action's cancel-service + status-publisher protocol types
    // (`action_msgs/srv/CancelGoal_{Request,Response}`,
    // `action_msgs/msg/GoalStatusArray`) are registered by the framework via
    // `RosAction::register_protocol_types` (generated impl), no example-side
    // registration needed.
    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("fibonacci_action_server");
    let mut executor = Executor::open(&cfg).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_action_server")
        .expect("Failed to create node");
    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");
    info!("Waiting for action goals...");

    loop {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            info!("Received goal request with order {}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                let _ = executor.spin_once(core::time::Duration::from_millis(10));
                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;
                    server.set_goal_status(&goal_id, GoalStatus::Executing);
                    info!("Executing goal");
                    let _ = executor.spin_once(core::time::Duration::from_millis(10));
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
                            info!("Publish feedback");
                        }
                        let _ = executor.spin_once(core::time::Duration::from_millis(10));
                        // Drain get_result queries during execution. A real
                        // `rcl_action` client (rclcpp_action) sends get_result
                        // right after acceptance; the server defers the reply
                        // until the goal terminates.
                        let _ = server.try_handle_get_result();
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    let result = FibonacciResult { sequence };
                    // `complete_goal` flushes any deferred get_result replies.
                    server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                    info!("Goal succeeded");
                    let _ = executor.spin_once(core::time::Duration::from_millis(10));
                }
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => error!("Error accepting goal: {:?}", e),
        }
        // Answer get_result requests that arrive between goals — including
        // one sent only after the goal completed (it hits the
        // completed-results path and is replied to immediately).
        let _ = server.try_handle_get_result();
    }
}
