//! Service response-info observer (issue 2136).
//!
//! # Why this is a bin and not the service-client example
//!
//! Issue 2136 gave the RMW C ABI a `take_response_with_info` slot and the Rust
//! client `Promise::take_with_info` / `wait_with_info`. The
//! `examples/*/service-client` programs are written against the
//! `Node`/`ExecutableNode` API, which hands a reply to a callback without its
//! `ServiceInfo`, and growing a test-shaped branch there would break the
//! phase-338 portability gate. So the observation lives here, as issue 2088's
//! `bins/service-info-server` does for the server side.
//!
//! # Output contract
//!
//! Calls `example_interfaces/srv/AddTwoInts` on `/add_two_ints`
//! `SERVICE_INFO_CLIENT_CALLS` times (default 3) with `a = i, b = 10`, each
//! call waited out with `Promise::wait_with_info`, and prints per reply:
//!
//! ```text
//! RESPONSE_INFO guid=<32 hex digits> seq=<n> source_ts=<ns|absent> received_ts=<ns|absent> sum=<sum>
//! ```
//!
//! (`nros_tests::output::RESPONSE_INFO_LOG_PREFIX`). A reply whose backend
//! reported no request id prints `RESPONSE_INFO ABSENT …` at error level
//! instead — never a zero GUID standing in for "unknown". After the last call
//! it prints `RESPONSE_INFO done`.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;

extern crate nros_platform_cffi as _;

fn ts(t: Option<nros::Time>) -> std::string::String {
    t.map(|t| t.to_nanos().to_string())
        .unwrap_or_else(|| "absent".into())
}

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens).
    service_info_client_nros_selection::nros_board_linux::register_linked_rmw();

    env_logger::init();

    let calls: i64 = std::env::var("SERVICE_INFO_CLIENT_CALLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    let ctx = nros::init().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_info_client");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let mut client = {
        let mut node = executor
            .create_node("add_two_ints_info_client")
            .expect("Failed to create node");
        node.create_client::<AddTwoInts>("/add_two_ints")
            .expect("Failed to create client")
    };

    // Wait for the server; a backend that cannot tell sends blind.
    match client.wait_for_service(&mut executor, core::time::Duration::from_secs(20)) {
        Ok(true) | Err(_) => {}
        Ok(false) => {
            error!("RESPONSE_INFO no server on /add_two_ints within 20 s");
            std::process::exit(1);
        }
    }

    for i in 0..calls {
        let request = AddTwoIntsRequest { a: i, b: 10 };
        let mut promise = client.call(&request).expect("Failed to send request");
        match promise.wait_with_info(&mut executor, core::time::Duration::from_secs(10)) {
            Ok((reply, info)) => match info.request_id {
                Some(id) => {
                    let guid = id
                        .writer_guid
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<std::string::String>();
                    info!(
                        "RESPONSE_INFO guid={guid} seq={} source_ts={} received_ts={} sum={}",
                        id.sequence_number,
                        ts(info.source_timestamp),
                        ts(info.received_timestamp),
                        reply.sum
                    );
                }
                None => error!(
                    "RESPONSE_INFO ABSENT — the backend reported no request id \
                     (source_ts={} received_ts={} sum={})",
                    ts(info.source_timestamp),
                    ts(info.received_timestamp),
                    reply.sum
                ),
            },
            Err(e) => {
                error!("RESPONSE_INFO call {i} failed: {e:?}");
                std::process::exit(1);
            }
        }
    }
    info!("RESPONSE_INFO done");
}
