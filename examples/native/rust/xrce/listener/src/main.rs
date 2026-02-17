//! XRCE-DDS listener — subscribes to Int32 on /chatter via XRCE Agent.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_MSG_COUNT   — Messages to receive before exiting (default: 5)

use nros::{EmbeddedConfig, EmbeddedExecutor};
use std::time::Instant;
use std_msgs::msg::Int32;

fn main() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let msg_count: usize = std::env::var("XRCE_MSG_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    eprintln!(
        "XRCE Listener: agent={}, domain={}, count={}",
        agent_addr, domain_id, msg_count
    );

    // Open session
    let config = EmbeddedConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_listener");
    let mut executor = EmbeddedExecutor::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create subscriber
    let mut node = executor
        .create_node("xrce_listener")
        .expect("Failed to create node");
    let mut subscription = node
        .create_subscription::<Int32>("/chatter")
        .expect("Failed to create subscriber");
    eprintln!("Subscriber created on /chatter");

    // Receiving loop
    println!("Waiting for messages...");
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(30);
    let mut received = 0usize;

    while received < msg_count && start.elapsed() < timeout {
        // Drive the XRCE session
        let _ = executor.drive_io(100);

        // Try to receive a typed message
        match subscription.try_recv() {
            Ok(Some(msg)) => {
                println!("Received: {}", msg.data);
                received += 1;
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("Receive error: {:?}", e);
            }
        }
    }

    if received >= msg_count {
        println!("Received {} messages, exiting", received);
    } else {
        eprintln!("Timeout: received only {}/{} messages", received, msg_count);
        std::process::exit(1);
    }

    // Clean up
    let _ = executor.close();
}
