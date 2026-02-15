//! XRCE-DDS service server — handles AddTwoInts requests via XRCE Agent.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_TIMEOUT     — Server timeout in seconds (default: 30)

use example_interfaces::srv::AddTwoInts;
use nros::xrce::*;
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

    // Initialize transport and open session
    init_posix_udp(&agent_addr);
    let mut executor =
        XrceExecutor::new("xrce_service_server", domain_id).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create service server
    let mut node = executor.create_node();
    let mut server = node
        .create_service_server::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create service server");
    eprintln!("Service server created on /add_two_ints");

    // Ready marker for test matching
    println!("Service server ready");

    // Request handling loop
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let mut req_buf = [0u8; 256];
    let mut reply_buf = [0u8; 256];

    while start.elapsed() < timeout {
        // Drive the XRCE session
        executor.spin_once(100);

        // Try to handle a request
        match server.handle_request(&mut req_buf, &mut reply_buf, |req| {
            println!("Received request: a={} b={}", req.a, req.b);
            let sum = req.a + req.b;
            let reply = example_interfaces::srv::AddTwoIntsResponse { sum };
            println!("Sent reply: sum={}", sum);
            reply
        }) {
            Ok(true) => {
                // Flush the reply
                executor.spin_once(100);
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("Handle request error: {}", e);
            }
        }
    }

    eprintln!("Server timeout, exiting");
    let _ = executor.close();
}
