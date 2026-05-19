//! XRCE-DDS service client — sends AddTwoInts requests via XRCE Agent.
//!
//! Uses the Promise API: `client.call()` returns immediately, then
//! `promise.wait()` drives I/O and waits for the reply.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR     — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID      — ROS domain ID (default: 0)
//!   XRCE_REQUEST_COUNT   — Number of requests to send (default: 3)

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::{Executor, ExecutorConfig};

use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-client");

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
    let request_count: usize = std::env::var("XRCE_REQUEST_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    nros_warn!(&LOGGER, 
        "XRCE Service Client: agent={}, domain={}, requests={}",
        agent_addr, domain_id, request_count
    );

    // Open session
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_service_client");
    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    nros_rmw_xrce_cffi::register().expect("Failed to register RMW backend");
    let mut executor = Executor::open(&config).expect("Failed to open XRCE session");
    nros_warn!(&LOGGER, "Session created");

    // Create service client
    let mut node = executor
        .create_node("xrce_service_client")
        .expect("Failed to create node");
    let mut client = node
        .create_client::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create service client");
    nros_warn!(&LOGGER, "Service client created for /add_two_ints");

    // Ready marker for test matching
    nros_info!(&LOGGER, "Service client ready");

    // Send requests using the Promise pattern
    let mut success_count = 0usize;

    for i in 0..request_count {
        let a = i as i64 + 1;
        let b = (i as i64 + 1) * 10;
        let request = AddTwoIntsRequest { a, b };

        nros_info!(&LOGGER, "Sent request: a={} b={}", a, b);

        // Non-blocking: send request and get a promise
        let mut promise = match client.call(&request) {
            Ok(p) => p,
            Err(e) => {
                nros_warn!(&LOGGER, "Failed to send request: {:?}", e);
                continue;
            }
        };

        // Wait for the reply (drives I/O internally)
        match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
            Ok(reply) => {
                nros_info!(&LOGGER, "Received reply: sum={}", reply.sum);
                success_count += 1;
            }
            Err(e) => {
                nros_warn!(&LOGGER, "Service call failed: {:?}", e);
            }
        }

        // Small delay between requests
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    nros_info!(&LOGGER, "Completed {}/{} requests", success_count, request_count);

    // Clean up
    let _ = executor.close();
}
