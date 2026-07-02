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

/// Action-client body — send a `Fibonacci` goal and collect feedback.
/// Returns 0 if any feedback arrived, 1 otherwise.
fn run() -> i32 {
    info!("nros Action Client Example");
    info!("================================");

    // Phase 244 E3 — CancelGoal / GoalStatusArray protocol types are registered
    // by the framework via `RosAction::register_protocol_types` (generated impl);
    // no example-side registration needed.
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
        Err(e) => warn!(
            "wait_for_action_server error: {:?} — sending goal anyway",
            e
        ),
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
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();
    std::process::exit(run());
}
