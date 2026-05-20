//! Native action-client example — shared logic for both build paths
//! (Phase 170.A). `run()` is shared by the pure-cargo `fn main()`
//! (zenoh/xrce) and the Cyclone DDS `rust_main()` C entry. Sends a
//! Fibonacci goal, waits for acceptance, then streams feedback.

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use log::{error, info, warn};
use nros::prelude::*;

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-cyclonedds", feature = "rmw-xrce")))]
compile_error!(
    "this example requires exactly one of `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce`",
);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    { nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?; }
    #[cfg(feature = "rmw-cyclonedds")]
    { nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?; }
    #[cfg(feature = "rmw-xrce")]
    { nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?; }
    Ok(())
}

/// Action-client body — send a `Fibonacci` goal and collect feedback.
/// Returns 0 if any feedback arrived, 1 otherwise.
pub fn run() -> i32 {
    info!("nros Action Client Example");
    info!("================================");

    if register_rmw().is_err() {
        error!("Failed to register RMW backend");
        return 1;
    }
    let config = ExecutorConfig::from_env().node_name("fibonacci_action_client");
    let mut executor = match Executor::open(&config) {
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

    // Warm up discovery before the first goal: send_goal is a service
    // call under the hood, and its request races the writer↔server-reader
    // endpoint match (same first-call race the service-client hits). The
    // action client exposes no `wait_for`/retry, so spin the executor for
    // a few seconds to let the endpoints match.
    let warmup = std::time::Instant::now();
    while warmup.elapsed() < std::time::Duration::from_millis(3000) {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));
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

#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    env_logger::init();
    run()
}

#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_platform_cffi as _;
