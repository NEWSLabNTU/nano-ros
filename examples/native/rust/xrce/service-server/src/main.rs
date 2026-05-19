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

use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-server");

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
    let timeout_secs: u64 = std::env::var("XRCE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    nros_warn!(&LOGGER, 
        "XRCE Service Server: agent={}, domain={}, timeout={}s",
        agent_addr, domain_id, timeout_secs
    );

    // Open session with callback arena
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_service_server");
    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_xrce_cffi::register().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");
    nros_warn!(&LOGGER, "Session created");

    // Register service callback
    executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |req| {
            nros_info!(&LOGGER, "Received request: a={} b={}", req.a, req.b);
            let sum = req.a + req.b;
            nros_info!(&LOGGER, "Sent reply: sum={}", sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    nros_warn!(&LOGGER, "Service server created on /add_two_ints");

    // Ready marker for test matching
    nros_info!(&LOGGER, "Service server ready");

    // Spin loop with timeout
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        executor.spin_once(core::time::Duration::from_millis(100));
    }

    nros_warn!(&LOGGER, "Server timeout, exiting");
    let _ = executor.close();
}
