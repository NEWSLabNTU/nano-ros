//! XRCE-DDS talker — publishes Int32 on /chatter via XRCE Agent.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)

use nros_core::RosMessage;
use nros_rmw::{Publisher, QosSettings, Rmw, RmwConfig, Session, SessionMode, TopicInfo};
use nros_rmw_xrce::XrceRmw;
use std_msgs::msg::Int32;
use xrce_native_test::init_posix_udp_transport;

fn main() {
    let agent_addr = std::env::var("XRCE_AGENT_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    eprintln!("XRCE Talker: agent={}, domain={}", agent_addr, domain_id);

    // Initialize transport
    unsafe {
        init_posix_udp_transport(&agent_addr);
    }

    // Open RMW session
    let config = RmwConfig {
        locator: &agent_addr,
        mode: SessionMode::Client,
        domain_id,
        node_name: "xrce_talker",
        namespace: "",
    };

    let mut session = XrceRmw::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create publisher
    let topic = TopicInfo::new("/chatter", Int32::TYPE_NAME, "");
    let publisher = session
        .create_publisher(&topic, QosSettings::RELIABLE)
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
                eprintln!("Publish error: {:?}", e);
            }
        }

        // Drive the XRCE session (flush output)
        session.spin_once(100);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Clean up
    let _ = session.close();
    eprintln!("Talker done");
}
