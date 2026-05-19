//! XRCE-DDS listener — subscribes to Int32 on /chatter via XRCE Agent.
//!
//! Uses the callback+spin pattern: registers a subscription callback, then
//! spins the executor which drives I/O and dispatches callbacks automatically.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_MSG_COUNT   — Messages to receive before exiting (default: 5)

use nros::{Executor, ExecutorConfig};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
use std_msgs::msg::Int32;

use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("listener");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let msg_count: usize = std::env::var("XRCE_MSG_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    nros_warn!(&LOGGER, 
        "XRCE Listener: agent={}, domain={}, count={}",
        agent_addr, domain_id, msg_count
    );

    // Open session with callback arena
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_listener");
    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_xrce_cffi::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");
    nros_warn!(&LOGGER, "Session created");

    // Register subscription callback
    let received = Arc::new(AtomicUsize::new(0));
    let received_cb = received.clone();
    executor
        .register_subscription::<Int32, _>("/chatter", move |msg: &Int32| {
            let n = received_cb.fetch_add(1, Ordering::SeqCst) + 1;
            nros_info!(&LOGGER, "[{}] Received: {}", n, msg.data);
        })
        .expect("Failed to add subscription");
    nros_warn!(&LOGGER, "Subscriber created on /chatter");

    // Spin loop with timeout
    nros_info!(&LOGGER, "Waiting for messages...");
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(30);

    while received.load(Ordering::SeqCst) < msg_count && start.elapsed() < timeout {
        executor.spin_once(core::time::Duration::from_millis(100));
    }

    let final_count = received.load(Ordering::SeqCst);
    if final_count >= msg_count {
        nros_info!(&LOGGER, "Received {} messages, exiting", final_count);
    } else {
        nros_warn!(&LOGGER, 
            "Timeout: received only {}/{} messages",
            final_count, msg_count
        );
        std::process::exit(1);
    }

    // Clean up
    let _ = executor.close();
}
