//! XRCE-DDS serial listener — subscribes to Int32 on /chatter via serial transport.
//!
//! Environment variables:
//!   XRCE_SERIAL_PTY  — PTY device path (required)
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_MSG_COUNT   — Messages to receive before exiting (default: 5)

use nros::xrce_transport::init_posix_serial;
use nros::{EmbeddedExecutor, Rmw, RmwConfig, SessionMode, XrceRmw};
use std::time::Instant;
use std_msgs::msg::Int32;

fn main() {
    let pty_path = std::env::var("XRCE_SERIAL_PTY")
        .expect("XRCE_SERIAL_PTY must be set to the PTY device path");
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let msg_count: usize = std::env::var("XRCE_MSG_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    eprintln!(
        "XRCE Serial Listener: pty={}, domain={}, count={}",
        pty_path, domain_id, msg_count
    );

    // Initialize transport and open session
    init_posix_serial(&pty_path);
    let config = RmwConfig {
        locator: &pty_path,
        mode: SessionMode::Client,
        domain_id,
        node_name: "xrce_serial_listener",
        namespace: "",
    };
    let session = XrceRmw::open(&config).expect("Failed to open XRCE session");
    let mut executor = EmbeddedExecutor::from_session(session);
    eprintln!("Session created");

    // Create subscriber
    let mut node = executor
        .create_node("xrce_serial_listener")
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
