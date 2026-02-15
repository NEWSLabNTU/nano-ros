//! XRCE-DDS service client — sends AddTwoInts requests via XRCE Agent.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR     — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID      — ROS domain ID (default: 0)
//!   XRCE_REQUEST_COUNT   — Number of requests to send (default: 3)

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::xrce::*;

fn main() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let request_count: usize = std::env::var("XRCE_REQUEST_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    eprintln!(
        "XRCE Service Client: agent={}, domain={}, requests={}",
        agent_addr, domain_id, request_count
    );

    // Initialize transport and open session
    init_posix_udp(&agent_addr);
    let mut executor = XrceExecutor::new("xrce_service_client", domain_id)
        .expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create service client
    let mut node = executor.create_node();
    let mut client = node
        .create_service_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create service client");
    eprintln!("Service client created for /add_two_ints");

    // Ready marker for test matching
    println!("Service client ready");

    // Send requests
    let mut req_buf = [0u8; 256];
    let mut reply_buf = [0u8; 256];
    let mut success_count = 0usize;

    for i in 0..request_count {
        let a = i as i64 + 1;
        let b = (i as i64 + 1) * 10;
        let request = AddTwoIntsRequest { a, b };

        println!("Sent request: a={} b={}", a, b);

        // Drive session before call to ensure connectivity
        executor.spin_once(100);

        match client.call(&request, &mut req_buf, &mut reply_buf) {
            Ok(reply) => {
                println!("Received reply: sum={}", reply.sum);
                success_count += 1;
            }
            Err(e) => {
                eprintln!("Service call error: {}", e);
            }
        }

        // Small delay between requests
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    println!("Completed {}/{} requests", success_count, request_count);

    // Clean up
    let _ = executor.close();
}
