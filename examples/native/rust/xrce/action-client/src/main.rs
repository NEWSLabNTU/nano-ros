//! XRCE-DDS action client — Fibonacci action via XRCE Agent.
//!
//! Uses the Promise API: `send_goal()` / `get_result()` return promises
//! that are resolved with `promise.wait()` which drives I/O internally.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR     — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID      — ROS domain ID (default: 0)
//!   XRCE_FIBONACCI_ORDER — Fibonacci sequence order to request (default: 5)

use nros::{Executor, ExecutorConfig};

use example_interfaces::action::{Fibonacci, FibonacciGoal};

use nros_log::{Logger, nros_debug, nros_error, nros_info, nros_trace, nros_warn};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-client");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let order: i32 = std::env::var("XRCE_FIBONACCI_ORDER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    nros_warn!(
        &LOGGER,
        "XRCE Action Client: agent={}, domain={}, order={}",
        agent_addr,
        domain_id,
        order
    );

    // Open session
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_action_client");
    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_xrce_cffi::register().expect("Failed to register RMW backend");
    let mut executor = Executor::open(&config).expect("Failed to open XRCE session");
    nros_warn!(&LOGGER, "Session created");

    // Create action client
    let mut node = executor
        .create_node("xrce_action_client")
        .expect("Failed to create node");
    let mut action_client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");

    nros_info!(&LOGGER, "Action client ready");

    // Send goal using the Promise pattern
    let goal = FibonacciGoal { order };
    let (goal_id, mut promise) = match action_client.send_goal(&goal) {
        Ok(pair) => pair,
        Err(e) => {
            nros_warn!(&LOGGER, "send_goal failed: {:?}", e);
            let _ = executor.close();
            return;
        }
    };

    // Wait for goal acceptance (drives I/O internally)
    let accepted = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok(accepted) => accepted,
        Err(e) => {
            nros_warn!(&LOGGER, "Goal acceptance failed: {:?}", e);
            let _ = executor.close();
            return;
        }
    };

    if !accepted {
        nros_info!(&LOGGER, "Goal rejected");
        let _ = executor.close();
        return;
    }
    nros_info!(&LOGGER, "Goal accepted: {:?}", goal_id);

    // Receive feedback via FeedbackStream (drives I/O internally, filters by goal ID)
    {
        let mut stream = action_client.feedback_stream_for(goal_id);
        let mut feedback_count = 0usize;
        for _ in 0..15 {
            // 15 x 1000ms = 15 second max
            match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
                Ok(Some(feedback)) => {
                    feedback_count += 1;
                    nros_info!(
                        &LOGGER,
                        "Feedback {}: sequence_len={}",
                        feedback_count,
                        feedback.sequence.len()
                    );

                    if feedback.sequence.len() as i32 > order {
                        nros_info!(&LOGGER, "All feedback received");
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    nros_warn!(&LOGGER, "Feedback receive error: {:?}", e);
                    break;
                }
            }
        }
    }

    // Small delay to let server finish storing result
    for _ in 0..5 {
        executor.spin_once(core::time::Duration::from_millis(100));
    }

    // Get result using the Promise pattern
    let mut result_promise = match action_client.get_result(&goal_id) {
        Ok(p) => p,
        Err(e) => {
            nros_warn!(&LOGGER, "get_result failed: {:?}", e);
            let _ = executor.close();
            return;
        }
    };

    match result_promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok((status, result)) => {
            nros_info!(
                &LOGGER,
                "Result: status={}, sequence={:?}",
                status,
                result.sequence
            );
        }
        Err(e) => {
            nros_warn!(&LOGGER, "get_result failed: {:?}", e);
        }
    }

    nros_info!(&LOGGER, "Action client done");
    let _ = executor.close();
}
