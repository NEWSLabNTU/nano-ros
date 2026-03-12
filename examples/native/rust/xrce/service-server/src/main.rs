//! XRCE-DDS service server — handles AddTwoInts requests via XRCE Agent.
//!
//! Uses the callback+spin pattern: registers a service callback, then
//! spins the executor which drives I/O and dispatches callbacks automatically.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_TIMEOUT     — Server timeout in seconds (default: 30)

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::{Executor, ExecutorConfig};
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

    // Open session with callback arena
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_service_server");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Register service callback
    executor
        .add_service::<AddTwoInts, _>("/add_two_ints", |req| {
            println!("Received request: a={} b={}", req.a, req.b);
            let sum = req.a + req.b;
            println!("Sent reply: sum={}", sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    eprintln!("Service server created on /add_two_ints");

    // Ready marker for test matching
    println!("Service server ready");

    // Spin loop with timeout
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        executor.spin_once(100);
    }

    eprintln!("Server timeout, exiting");
    let _ = executor.close();
}
