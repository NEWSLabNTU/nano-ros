//! XRCE-DDS talker — publishes Int32 on /chatter via XRCE Agent.
//!
//! Uses the timer+spin pattern: registers a timer callback that publishes
//! messages periodically, then spins the executor.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)

use nros::{Executor, ExecutorConfig, TimerDuration};
use std::sync::{
    Arc,
    atomic::{AtomicI32, Ordering},
};
use std_msgs::msg::Int32;

fn main() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    eprintln!("XRCE Talker: agent={}, domain={}", agent_addr, domain_id);

    // Open session with callback arena
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_talker");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create publisher
    let mut node = executor
        .create_node("xrce_talker")
        .expect("Failed to create node");
    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");
    eprintln!("Publisher created on /chatter");

    // Register timer callback that publishes every 500ms
    println!("Publishing Int32 messages...");
    let counter = Arc::new(AtomicI32::new(0));
    let counter_cb = counter.clone();
    executor
        .add_timer(TimerDuration::from_millis(500), move || {
            let i = counter_cb.fetch_add(1, Ordering::SeqCst);
            match publisher.publish(&Int32 { data: i }) {
                Ok(()) => println!("Published: {}", i),
                Err(e) => eprintln!("Publish error: {:?}", e),
            }
        })
        .expect("Failed to add timer");

    // Spin until 20 messages published
    while counter.load(Ordering::SeqCst) < 20 {
        executor.spin_once(core::time::Duration::from_millis(100));
    }

    // Clean up
    let _ = executor.close();
    eprintln!("Talker done");
}
