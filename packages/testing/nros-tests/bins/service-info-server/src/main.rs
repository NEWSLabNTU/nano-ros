//! Service request-info observer (issue 2088).
//!
//! # Why this is a bin and not the service-server example
//!
//! Issue 2081 gave the RMW C ABI a `take_request_with_info` slot, and the XRCE
//! backend fills it from the Agent's `SampleIdentity`. Nothing could observe
//! that answer end to end: the `examples/*/service-server` programs are written
//! against the `Node`/`ExecutableNode` API, whose `CallbackCtx` exposes no
//! `ServiceInfo`, and growing a test-shaped branch there would break the
//! phase-338 portability gate (every platform copy byte-identical). The
//! rclrs-shaped `(request, &ServiceInfo)` callback lives on the executor's
//! `create_service_with_info`, so the observation moves here, as issue 0441
//! did for `MessageInfo` (`bins/message-info-observer`).
//!
//! # Output contract
//!
//! Serves `example_interfaces/srv/AddTwoInts` on `/add_two_ints` and replies
//! `sum = a + b`, so a stock `ros2 service call` reads it like the demo server.
//! It prints the demo server's readiness marker
//! (`nros_tests::output::SERVICE_SERVER_READY_MARKER`) and then, per request:
//!
//! ```text
//! REQUEST_ID guid=<32 hex digits> seq=<client sequence number> a=<a> b=<b> received_ts=<ns|absent>
//! ```
//!
//! (`nros_tests::output::SERVICE_INFO_LOG_PREFIX`). A request whose backend
//! reported no request id prints `REQUEST_ID ABSENT …` at error level instead,
//! never a zero GUID standing in for "unknown" — a zero is what a broken fill
//! looks like, so it must not also be what "not reported" looks like.
//!
//! `SERVICE_INFO_SERVER_QOS` (`default` | `best_effort`) picks the service
//! QoS, so a stock peer can read back the profile both endpoints were created
//! with (issue 2120).

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use log::{error, info};
use nros::prelude::*;

extern crate nros_platform_cffi as _;

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens).
    service_info_server_nros_selection::nros_board_linux::register_linked_rmw();

    env_logger::init();

    let ctx = nros::init().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_server");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let nid = executor
        .node_builder("add_two_ints_server")
        .build()
        .expect("Failed to build node");

    // Issue 2120 — the service QoS, so a stock peer can read back what the
    // endpoints were created with. `default` (or unset) is the services
    // default; `best_effort` changes reliability only.
    let qos = match std::env::var("SERVICE_INFO_SERVER_QOS").as_deref() {
        Ok("best_effort") => QoSProfile::services_default().best_effort(),
        Ok("default") | Err(_) => QoSProfile::services_default(),
        Ok(other) => panic!("SERVICE_INFO_SERVER_QOS={other}: expected `default` or `best_effort`"),
    };

    executor
        .node_mut(nid)
        .service("/add_two_ints")
        .service_info()
        .qos(qos)
        .build::<AddTwoInts, _>(
            |req: &AddTwoIntsRequest, info: &nros::ServiceInfo| {
                match info.request_id {
                    Some(id) => {
                        let guid = id
                            .writer_guid
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<std::string::String>();
                        // Issue 2135 — when the backend RECEIVED the request,
                        // wall-clock ns, or `absent` when it reported none.
                        let received = info
                            .received_timestamp
                            .map(|t| t.to_nanos().to_string())
                            .unwrap_or_else(|| "absent".into());
                        info!(
                            "REQUEST_ID guid={guid} seq={} a={} b={} received_ts={received}",
                            id.sequence_number, req.a, req.b
                        );
                    }
                    None => error!(
                        "REQUEST_ID ABSENT — the backend reported no request id \
                         (a={} b={})",
                        req.a, req.b
                    ),
                }
                AddTwoIntsResponse { sum: req.a + req.b }
            },
        )
        .expect("Failed to create service");

    info!("Waiting for service requests");

    if let Err(e) = executor.spin(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
