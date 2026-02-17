//! XRCE-DDS action server — Fibonacci action via XRCE Agent.
//!
//! Uses the typed `EmbeddedActionServer` API (no raw CDR needed).
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_TIMEOUT     — Server timeout in seconds (default: 30)

use nros::{EmbeddedConfig, EmbeddedExecutor, GoalResponse, GoalStatus};
use std::time::Instant;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};

fn main() {
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

    eprintln!(
        "XRCE Action Server: agent={}, domain={}, timeout={}s",
        agent_addr, domain_id, timeout_secs
    );

    // Open session
    let config = EmbeddedConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_action_server");
    let mut executor = EmbeddedExecutor::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create action server
    let mut node = executor
        .create_node("xrce_action_server")
        .expect("Failed to create node");
    let mut action_server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");

    println!("Action server ready");

    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        let _ = executor.drive_io(100);

        // Handle get_result requests
        let _ = action_server.try_handle_get_result();

        // Try to accept a new goal
        let accepted = action_server
            .try_accept_goal(|goal| {
                println!("Received goal: order={}", goal.order);
                GoalResponse::AcceptAndExecute
            })
            .expect("accept error");

        if let Some(goal_id) = accepted {
            let order = match action_server.get_goal(&goal_id) {
                Some(g) => g.goal.order,
                None => continue,
            };

            println!("Goal accepted: {:?}", goal_id);
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

                println!("Feedback: step={}, sequence_len={}", i, sequence.len());
                let _ = executor.drive_io(100);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            // Complete the goal
            let result = FibonacciResult { sequence };
            println!("Goal completed: result_len={}", result.sequence.len());
            action_server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
        }
    }

    eprintln!("Server timeout, exiting");
    let _ = executor.close();
}
