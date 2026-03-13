//! XRCE-DDS serial listener — subscribes to Int32 on /chatter via serial transport.
//!
//! Uses the callback+spin pattern: registers a subscription callback, then
//! spins the executor which drives I/O and dispatches callbacks automatically.
//!
//! Environment variables:
//!   XRCE_SERIAL_PTY  — PTY device path (required)
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_MSG_COUNT   — Messages to receive before exiting (default: 5)

use nros::{Executor, ExecutorConfig};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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

    // Open session with callback arena
    let config = ExecutorConfig::new(&pty_path)
        .domain_id(domain_id)
        .node_name("xrce_serial_listener");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Register subscription callback
    let received = Arc::new(AtomicUsize::new(0));
    let received_cb = received.clone();
    executor
        .add_subscription::<Int32, _>("/chatter", move |msg: &Int32| {
            let n = received_cb.fetch_add(1, Ordering::SeqCst) + 1;
            println!("[{}] Received: {}", n, msg.data);
        })
        .expect("Failed to add subscription");
    eprintln!("Subscriber created on /chatter");

    // Spin loop with timeout
    println!("Waiting for messages...");
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(30);

    while received.load(Ordering::SeqCst) < msg_count && start.elapsed() < timeout {
        executor.spin_once(100);
    }

    let final_count = received.load(Ordering::SeqCst);
    if final_count >= msg_count {
        println!("Received {} messages, exiting", final_count);
    } else {
        eprintln!(
            "Timeout: received only {}/{} messages",
            final_count, msg_count
        );
        std::process::exit(1);
    }

    // Clean up
    let _ = executor.close();
}
