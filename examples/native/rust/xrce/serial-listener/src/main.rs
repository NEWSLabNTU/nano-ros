//! XRCE-DDS serial listener — subscribes to Int32 on /chatter via serial transport.
//!
//! Environment variables:
//!   XRCE_SERIAL_PTY  — PTY device path (required)
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_MSG_COUNT   — Messages to receive before exiting (default: 5)

use nros_core::RosMessage;
use nros_rmw::{QosSettings, Rmw, RmwConfig, Session, SessionMode, Subscriber, TopicInfo};
use nros_rmw_xrce::XrceRmw;
use nros_rmw_xrce::posix_serial::init_posix_serial_transport;
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

    // Initialize serial transport
    unsafe {
        init_posix_serial_transport(&pty_path);
    }

    // Open RMW session
    let config = RmwConfig {
        locator: &pty_path,
        mode: SessionMode::Client,
        domain_id,
        node_name: "xrce_serial_listener",
        namespace: "",
    };

    let mut session = XrceRmw::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create subscriber
    let topic = TopicInfo::new("/chatter", Int32::TYPE_NAME, "");
    let mut subscriber = session
        .create_subscriber(&topic, QosSettings::RELIABLE)
        .expect("Failed to create subscriber");
    eprintln!("Subscriber created on /chatter");

    // Receiving loop
    println!("Waiting for messages...");
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(30);
    let mut received = 0usize;
    let mut buf = [0u8; 256];

    while received < msg_count && start.elapsed() < timeout {
        // Drive the XRCE session
        session.spin_once(100);

        // Try to receive a typed message
        match subscriber.try_recv::<Int32>(&mut buf) {
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
    let _ = session.close();
}
