//! XRCE-DDS serial talker — publishes Int32 on /chatter via serial transport.
//!
//! Environment variables:
//!   XRCE_SERIAL_PTY  — PTY device path (required)
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)

use nros::xrce::*;
use std_msgs::msg::Int32;

fn main() {
    let pty_path = std::env::var("XRCE_SERIAL_PTY")
        .expect("XRCE_SERIAL_PTY must be set to the PTY device path");
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    eprintln!("XRCE Serial Talker: pty={}, domain={}", pty_path, domain_id);

    // Initialize transport and open session
    init_posix_serial(&pty_path);
    let mut executor =
        XrceExecutor::new("xrce_serial_talker", domain_id).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create publisher
    let mut node = executor.create_node();
    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");
    eprintln!("Publisher created on /chatter");

    // Publishing loop
    println!("Publishing Int32 messages...");
    let mut buf = [0u8; 256];
    for i in 0i32..20 {
        let msg = Int32 { data: i };
        match publisher.publish(&msg, &mut buf) {
            Ok(()) => {
                println!("Published: {}", i);
            }
            Err(e) => {
                eprintln!("Publish error: {}", e);
            }
        }

        // Drive the XRCE session (flush output)
        executor.spin_once(100);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Clean up
    let _ = executor.close();
    eprintln!("Talker done");
}
