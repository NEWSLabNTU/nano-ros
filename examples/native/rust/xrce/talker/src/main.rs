//! XRCE-DDS talker — publishes Int32 on /chatter via XRCE Agent.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)

use nros::xrce_transport::init_posix_udp;
use nros::{EmbeddedExecutor, Rmw, RmwConfig, SessionMode, XrceRmw};
use std_msgs::msg::Int32;

fn main() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    eprintln!("XRCE Talker: agent={}, domain={}", agent_addr, domain_id);

    // Initialize transport and open session
    init_posix_udp(&agent_addr);
    let config = RmwConfig {
        locator: &agent_addr,
        mode: SessionMode::Client,
        domain_id,
        node_name: "xrce_talker",
        namespace: "",
    };
    let session = XrceRmw::open(&config).expect("Failed to open XRCE session");
    let mut executor = EmbeddedExecutor::from_session(session);
    eprintln!("Session created");

    // Create publisher
    let mut node = executor
        .create_node("xrce_talker")
        .expect("Failed to create node");
    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");
    eprintln!("Publisher created on /chatter");

    // Publishing loop
    println!("Publishing Int32 messages...");
    for i in 0i32..20 {
        let msg = Int32 { data: i };
        match publisher.publish(&msg) {
            Ok(()) => {
                println!("Published: {}", i);
            }
            Err(e) => {
                eprintln!("Publish error: {:?}", e);
            }
        }

        // Drive the XRCE session (flush output)
        let _ = executor.drive_io(100);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Clean up
    let _ = executor.close();
    eprintln!("Talker done");
}
