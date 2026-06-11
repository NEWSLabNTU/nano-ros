//! Native Action Client — Phase 212.L.2 Application pkg shape.
//!
//! Sends a `Fibonacci` goal, waits for acceptance, then streams
//! feedback. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] (Pattern 2) then a user-owned spin
//! loop.
//!
//! ```bash
//! cargo run -p native-rs-action-server   # then this client
//! cargo run -p native-rs-action-client
//! ```

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use log::{error, info, warn};
use nros::prelude::*;

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!("this example requires exactly one of `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce`",);

// Phase 227.3 (unified RMW) — no explicit `register()` calls. The RMW is
// declared via the build feature (`rmw-zenoh` / `rmw-xrce` / `rmw-cyclonedds`),
// which routes through the `nros` umbrella; `nros`'s `#[used] __FORCE_LINK_*`
// statics keep the selected backend's self-register section in the link graph,
// and it fires inside `nros::init` via the cffi-rmw walker.

/// Action-client body — send a `Fibonacci` goal and collect feedback.
/// Returns 0 if any feedback arrived, 1 otherwise.
fn run() -> i32 {
    info!("nros Action Client Example");
    info!("================================");

    // Phase 212.K.7.4.c — see action-server's main for rationale.
    #[cfg(feature = "rmw-cyclonedds")]
    {
        if nros_rmw_cyclonedds::register::<action_msgs::srv::CancelGoalRequest>().is_err()
            || nros_rmw_cyclonedds::register::<action_msgs::srv::CancelGoalResponse>().is_err()
            || nros_rmw_cyclonedds::register::<action_msgs::msg::GoalStatusArray>().is_err()
        {
            error!("Failed to register CancelGoal / GoalStatusArray types");
            return 1;
        }
    }

    let ctx = match nros::init_with_launch_auto() {
        Ok(c) => c,
        Err(_) => return 1,
    };
    let cfg = ctx.config("fibonacci_action_client");
    let mut executor = match Executor::open(&cfg) {
        Ok(e) => e,
        Err(_) => return 1,
    };

    let mut node = match executor.create_node("fibonacci_action_client") {
        Ok(n) => n,
        Err(_) => return 1,
    };
    info!("Node created: fibonacci_action_client");
    let mut client = match node.create_action_client::<Fibonacci>("/fibonacci") {
        Ok(c) => c,
        Err(_) => {
            error!("Failed to create action client");
            return 1;
        }
    };
    info!("Action client created: /fibonacci");

    // Wait for the action server before the first goal: send_goal is a
    // service call under the hood, and its request races the
    // writer↔server-reader endpoint match (same first-call race the
    // service-client hits) — a request published before the match is lost
    // under VOLATILE durability. `wait_for_action_server` spins the executor
    // while probing send_goal-server reachability, so it both drives
    // discovery and blocks until the endpoints match. Proceed anyway on
    // timeout (backends without liveliness probing fall back to the spin).
    match client.wait_for_action_server(&mut executor, core::time::Duration::from_secs(10)) {
        Ok(true) => info!("Action server discovered"),
        Ok(false) => warn!("Action server not confirmed within 10s — sending goal anyway"),
        Err(e) => warn!("wait_for_action_server error: {:?} — sending goal anyway", e),
    }

    let goal = FibonacciGoal { order: 10 };
    info!("Sending goal: order={}", goal.order);
    let (goal_id, mut promise) = match client.send_goal(&goal) {
        Ok(pair) => pair,
        Err(e) => {
            error!("Failed to send goal: {:?}", e);
            return 1;
        }
    };

    let accepted = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok(accepted) => accepted,
        Err(e) => {
            error!("Goal acceptance failed: {:?}", e);
            return 1;
        }
    };
    if !accepted {
        warn!("Goal was rejected by the server");
        return 1;
    }
    info!("Goal accepted! ID: {:?}", goal_id);
    info!("Waiting for feedback...");

    let mut stream = client.feedback_stream_for(goal_id);
    let mut feedback_count = 0;
    for _ in 0..30 {
        match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
            Ok(Some(feedback)) => {
                feedback_count += 1;
                info!("Feedback #{}: {:?}", feedback_count, feedback.sequence);
                if feedback.sequence.len() as i32 > goal.order {
                    info!("Final sequence: {:?}", feedback.sequence);
                    break;
                }
            }
            Ok(None) => {}
            Err(e) => {
                error!("Error receiving feedback: {:?}", e);
                break;
            }
        }
    }
    info!("Action client finished ({} feedback msgs)", feedback_count);
    if feedback_count > 0 { 0 } else { 1 }
}

fn main() {
    env_logger::init();
    std::process::exit(run());
}
