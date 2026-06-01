//! Native Action Server — Phase 212.L.2 Application pkg shape.
//!
//! Serves the `Fibonacci` action on `/fibonacci`, publishing feedback as
//! it computes. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] (Pattern 2) then a user-owned spin
//! loop.
//!
//! ```bash
//! cargo run -p native-rs-action-server   # then native-rs-action-client
//! ```

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use log::{error, info};
use nros::prelude::*;

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!("this example requires exactly one of `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce`",);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?;
    }
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    Ok(())
}

fn main() -> ! {
    env_logger::init();
    info!("nros Action Server Example");
    info!("================================");

    register_rmw().expect("Failed to register RMW backend");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("fibonacci_action_server");
    let mut executor = Executor::open(&cfg).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_action_server")
        .expect("Failed to create node");
    info!("Node created: fibonacci_action_server");
    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");
    info!("Action server created: /fibonacci");
    info!("Waiting for action goals...");

    loop {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            info!("Received goal request: order={}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                info!("Goal accepted: {}", goal_id);
                let _ = executor.spin_once(core::time::Duration::from_millis(10));
                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;
                    server.set_goal_status(&goal_id, GoalStatus::Executing);
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
                            info!("Feedback: {:?}", feedback.sequence);
                        }
                        let _ = executor.spin_once(core::time::Duration::from_millis(10));
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    let result = FibonacciResult { sequence };
                    info!("Goal completed: {:?}", result.sequence);
                    server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                    let _ = executor.spin_once(core::time::Duration::from_millis(10));
                }
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => error!("Error accepting goal: {:?}", e),
        }
    }
}
