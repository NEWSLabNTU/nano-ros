//! XRCE-DDS service server — handles AddTwoInts requests via XRCE Agent.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_TIMEOUT     — Server timeout in seconds (default: 30)

use example_interfaces::srv::AddTwoInts;
use nros::{EmbeddedConfig, EmbeddedExecutor};
use std::time::Instant;

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
        "XRCE Service Server: agent={}, domain={}, timeout={}s",
        agent_addr, domain_id, timeout_secs
    );

    // Open session
    let config = EmbeddedConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_service_server");
    let mut executor = EmbeddedExecutor::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create service server
    let mut node = executor
        .create_node("xrce_service_server")
        .expect("Failed to create node");
    let mut server = node
        .create_service::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create service server");
    eprintln!("Service server created on /add_two_ints");

    // Ready marker for test matching
    println!("Service server ready");

    // Request handling loop
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        // Drive the XRCE session
        let _ = executor.drive_io(100);

        // Try to handle a request
        match server.handle_request(|req| {
            println!("Received request: a={} b={}", req.a, req.b);
            let sum = req.a + req.b;
            let reply = example_interfaces::srv::AddTwoIntsResponse { sum };
            println!("Sent reply: sum={}", sum);
            reply
        }) {
            Ok(true) => {
                // Flush the reply
                let _ = executor.drive_io(100);
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("Handle request error: {:?}", e);
            }
        }
    }

    eprintln!("Server timeout, exiting");
    let _ = executor.close();
}
