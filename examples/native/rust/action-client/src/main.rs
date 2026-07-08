//! Native Action Client.
//!
//! Sends a `Fibonacci` goal of order 10, streams feedback, then fetches the
//! result — matching the official ROS 2 `action_tutorials`
//! `fibonacci_action_client` demo. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] then a user-owned spin loop.
//!
//! ```bash
//! cargo run -p native-rs-action-server   # then this client
//! cargo run -p native-rs-action-client
//! ```

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use log::{error, info, warn};
use nros::prelude::*;

/// Action-client body — send a `Fibonacci` goal, stream feedback, fetch the
/// result. Returns the process exit code.
fn run() -> i32 {
    // CancelGoal / GoalStatusArray protocol types are registered by the
    // framework via `RosAction::register_protocol_types` (generated impl);
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
    let mut client = match node.create_action_client::<Fibonacci>("/fibonacci") {
        Ok(c) => c,
        Err(_) => {
            error!("Failed to create action client");
            return 1;
        }
    };

    // Wait for the action server before the first goal: send_goal is a
    // service call under the hood, and its request races the
    // writer↔server-reader endpoint match (same first-call race the
    // service-client hits) — a request published before the match is lost
    // under VOLATILE durability. `wait_for_action_server` spins the executor
    // while probing send_goal-server reachability, so it both drives
    // discovery and blocks until the endpoints match. Proceed anyway on
    // timeout (backends without liveliness probing fall back to the spin).
    match client.wait_for_action_server(&mut executor, core::time::Duration::from_secs(10)) {
        Ok(true) => {}
        Ok(false) => warn!("Action server not confirmed within 10s — sending goal anyway"),
        Err(e) => warn!(
            "wait_for_action_server error: {:?} — sending goal anyway",
            e
        ),
    }

    let goal = FibonacciGoal { order: 10 };
    // Issue 0153 — retry the goal handshake with a 1 s backoff. On rmw_zenoh
    // the server's liveliness token (what `wait_for_action_server` observes)
    // gossips ahead of its queryable route; a send_goal fired in that window
    // matches no queryable and times out instantly. Same fix shape as the
    // service-client demo.
    let mut accepted_goal = None;
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(core::time::Duration::from_secs(1));
        }
        info!("Sending goal");
        let (goal_id, mut promise) = match client.send_goal(&goal) {
            Ok(pair) => pair,
            Err(e) => {
                error!("Failed to send goal: {:?}", e);
                return 1;
            }
        };
        match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
            Ok(true) => {
                accepted_goal = Some(goal_id);
                break;
            }
            Ok(false) => {
                warn!("Goal was rejected by the server");
                return 1;
            }
            Err(e) => {
                error!("Goal acceptance failed (attempt {}): {:?}", attempt + 1, e);
                // A timed-out acceptance promise leaves the send-goal
                // in-flight flag set; clear it or the retry dies on
                // RequestInFlight (same contract as the service client's
                // reset_in_flight).
                client.reset_send_goal_in_flight();
            }
        }
    }
    let Some(goal_id) = accepted_goal else {
        error!("Goal was never accepted");
        return 1;
    };
    info!("Goal accepted by server, waiting for result");

    // Stream feedback until the sequence is complete (the server publishes
    // the growing sequence once per step).
    let mut stream = client.feedback_stream_for(goal_id);
    for _ in 0..30 {
        match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
            Ok(Some(feedback)) => {
                info!("Next number in sequence received: {:?}", feedback.sequence);
                if feedback.sequence.len() as i32 > goal.order {
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

    // Fetch the terminal result (replied immediately once the goal
    // completed — the server holds earlier get_result requests until then).
    let mut result_promise = match client.get_result(&goal_id) {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to request result: {:?}", e);
            return 1;
        }
    };
    match result_promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok((status, result)) => {
            if status != GoalStatus::Succeeded {
                warn!("Goal finished with status {:?}", status);
            }
            info!("Result received: {:?}", result.sequence);
            0
        }
        Err(e) => {
            error!("Failed to get result: {:?}", e);
            1
        }
    }
}

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();
    std::process::exit(run());
}
