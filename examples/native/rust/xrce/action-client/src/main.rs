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
use std::time::Instant;

use example_interfaces::action::{Fibonacci, FibonacciGoal};

fn main() {
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

    eprintln!(
        "XRCE Action Client: agent={}, domain={}, order={}",
        agent_addr, domain_id, order
    );

    // Open session
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_action_client");
    let mut executor = Executor::<_, 0, 0>::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create action client
    let mut node = executor
        .create_node("xrce_action_client")
        .expect("Failed to create node");
    let mut action_client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");

    println!("Action client ready");

    // Send goal using the Promise pattern
    let goal = FibonacciGoal { order };
    let (goal_id, mut promise) = match action_client.send_goal(&goal) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("send_goal failed: {:?}", e);
            let _ = executor.close();
            return;
        }
    };

    // Wait for goal acceptance (drives I/O internally)
    let accepted = match promise.wait(&mut executor, 10000) {
        Ok(accepted) => accepted,
        Err(e) => {
            eprintln!("Goal acceptance failed: {:?}", e);
            let _ = executor.close();
            return;
        }
    };

    if !accepted {
        println!("Goal rejected");
        let _ = executor.close();
        return;
    }
    println!("Goal accepted: {:?}", goal_id);

    // Wait for feedback
    let mut feedback_count = 0usize;
    let start = Instant::now();
    let feedback_timeout = std::time::Duration::from_secs(15);

    while start.elapsed() < feedback_timeout {
        executor.spin_once(100);

        match action_client.try_recv_feedback() {
            Ok(Some((_fid, feedback))) => {
                feedback_count += 1;
                println!(
                    "Feedback {}: sequence_len={}",
                    feedback_count,
                    feedback.sequence.len()
                );

                if feedback.sequence.len() as i32 > order {
                    println!("All feedback received");
                    break;
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("Feedback receive error: {:?}", e);
            }
        }
    }

    // Small delay to let server finish storing result
    for _ in 0..5 {
        executor.spin_once(100);
    }

    // Get result using the Promise pattern
    let mut result_promise = match action_client.get_result(&goal_id) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("get_result failed: {:?}", e);
            let _ = executor.close();
            return;
        }
    };

    match result_promise.wait(&mut executor, 10000) {
        Ok((status, result)) => {
            println!("Result: status={}, sequence={:?}", status, result.sequence);
        }
        Err(e) => {
            eprintln!("get_result failed: {:?}", e);
        }
    }

    println!("Action client done");
    let _ = executor.close();
}
