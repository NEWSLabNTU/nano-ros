//! XRCE-DDS action server — Fibonacci action via XRCE Agent.
//!
//! Uses the typed `ActionServer` API (no raw CDR needed).
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_TIMEOUT     — Server timeout in seconds (default: 30)

use nros::{Executor, ExecutorConfig, GoalResponse, GoalStatus};
use std::time::Instant;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};

use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-server");

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
    let timeout_secs: u64 = std::env::var("XRCE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    nros_warn!(&LOGGER, 
        "XRCE Action Server: agent={}, domain={}, timeout={}s",
        agent_addr, domain_id, timeout_secs
    );

    // Open session
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_action_server");
    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_xrce_cffi::register().expect("Failed to register RMW backend");
    let mut executor = Executor::open(&config).expect("Failed to open XRCE session");
    nros_warn!(&LOGGER, "Session created");

    // Create action server
    let mut node = executor
        .create_node("xrce_action_server")
        .expect("Failed to create node");
    let mut action_server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");

    nros_info!(&LOGGER, "Action server ready");

    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        executor.spin_once(core::time::Duration::from_millis(100));

        // Handle get_result requests
        let _ = action_server.try_handle_get_result();

        // Try to accept a new goal
        let accepted = action_server
            .try_accept_goal(|_goal_id, goal| {
                nros_info!(&LOGGER, "Received goal: order={}", goal.order);
                GoalResponse::AcceptAndExecute
            })
            .expect("accept error");

        if let Some(goal_id) = accepted {
            let order = match action_server.get_goal(&goal_id) {
                Some(g) => g.goal.order,
                None => continue,
            };

            nros_info!(&LOGGER, "Goal accepted: {:?}", goal_id);
            action_server.set_goal_status(&goal_id, GoalStatus::Executing);

            // Execute Fibonacci computation with feedback
            let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();

            for i in 0..=order {
                let val = if i <= 1 {
                    i
                } else {
                    let n = sequence.len();
                    sequence[n - 1] + sequence[n - 2]
                };
                let _ = sequence.push(val);

                // Publish feedback
                let feedback = FibonacciFeedback {
                    sequence: sequence.clone(),
                };
                let _ = action_server.publish_feedback(&goal_id, &feedback);

                nros_info!(&LOGGER, "Feedback: step={}, sequence_len={}", i, sequence.len());
                executor.spin_once(core::time::Duration::from_millis(100));
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            // Complete the goal
            let result = FibonacciResult { sequence };
            nros_info!(&LOGGER, "Goal completed: result_len={}", result.sequence.len());
            action_server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
        }
    }

    nros_warn!(&LOGGER, "Server timeout, exiting");
    let _ = executor.close();
}
