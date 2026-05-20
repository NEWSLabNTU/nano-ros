//! Native Rust action client over the Cyclone DDS RMW backend.
//!
//! Phase 171.0.b — native rust cyclonedds is cmake-driven: this crate
//! compiles to a `staticlib` named `rustapp` exposing a C `rust_main()`
//! entry. The per-example `CMakeLists.txt` runs
//! `nros_generate_interfaces(...)` for example_interfaces + action_msgs
//! + unique_identifier_msgs + builtin_interfaces, builds the C++
//! `nros-rmw-cyclonedds` backend, and links both alongside this
//! staticlib + `libddsc` + `stdc++`.

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::prelude::*;
use nros_log::{nros_error, nros_info, nros_warn, Logger};

static LOGGER: Logger = Logger::new("action-client");

extern crate nros_platform_cffi as _;

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    if nros_rmw_cyclonedds_sys::register().is_err() {
        nros_error!(&LOGGER, "Failed to register Cyclone DDS RMW backend");
        return 1;
    }
    nros_info!(&LOGGER, "nros Native Action Client (Cyclone DDS Transport)");

    let config = ExecutorConfig::from_env().node_name("fibonacci_action_client");
    let mut executor: Executor = match Executor::open(&config) {
        Ok(e) => e,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to open executor");
            return 1;
        }
    };

    let mut node = match executor.create_node("fibonacci_action_client") {
        Ok(n) => n,
        Err(_) => return 1,
    };
    let mut client = match node.create_action_client::<Fibonacci>("/fibonacci") {
        Ok(c) => c,
        Err(_) => {
            nros_error!(&LOGGER, "Failed to create action client");
            return 1;
        }
    };
    nros_info!(&LOGGER, "Action client created: /fibonacci");

    // Warm up discovery before the first goal: send_goal is a service
    // call under the hood, and its request races the writer↔server-reader
    // endpoint match (same first-call race the service-client hits). The
    // action client exposes no `wait_for`/retry, so spin the executor for
    // a few seconds to let the endpoints match — mirrors the C client.
    let warmup = std::time::Instant::now();
    while warmup.elapsed() < std::time::Duration::from_millis(3000) {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));
    }

    let goal = FibonacciGoal { order: 10 };
    nros_info!(&LOGGER, "Sending goal: order={}", goal.order);

    let (goal_id, mut promise) = match client.send_goal(&goal) {
        Ok(pair) => pair,
        Err(e) => {
            nros_error!(&LOGGER, "Failed to send goal: {:?}", e);
            return 1;
        }
    };

    let accepted = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok(accepted) => accepted,
        Err(e) => {
            nros_error!(&LOGGER, "Goal acceptance failed: {:?}", e);
            return 1;
        }
    };
    if !accepted {
        nros_warn!(&LOGGER, "Goal was rejected by the server");
        return 1;
    }
    nros_info!(&LOGGER, "Goal accepted! ID: {:?}", goal_id);
    nros_info!(&LOGGER, "Waiting for feedback...");

    let mut stream = client.feedback_stream_for(goal_id);
    let mut feedback_count = 0;
    for _ in 0..30 {
        match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
            Ok(Some(feedback)) => {
                feedback_count += 1;
                nros_info!(&LOGGER, "Feedback #{}: {:?}", feedback_count, feedback.sequence);
                if feedback.sequence.len() as i32 > goal.order {
                    nros_info!(&LOGGER, "Final sequence: {:?}", feedback.sequence);
                    break;
                }
            }
            Ok(None) => {}
            Err(e) => {
                nros_error!(&LOGGER, "Error receiving feedback: {:?}", e);
                break;
            }
        }
    }
    nros_info!(&LOGGER, "Action client finished ({} feedback msgs)", feedback_count);
    if feedback_count > 0 {
        0
    } else {
        1
    }
}
