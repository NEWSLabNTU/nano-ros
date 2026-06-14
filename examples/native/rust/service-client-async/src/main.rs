//! Async Service Client Example — tokio background spin
//!
//! Demonstrates the background spin pattern for async service calls:
//! 1. Create executor → create client (owned, no lifetime to executor)
//! 2. Move executor to a background `spin_async()` task via `spawn_local`
//! 3. `.await` Promises directly from the main task
//!
//! This pattern uses tokio's `current_thread` runtime with `LocalSet` for
//! single-threaded cooperative concurrency — no multi-threading needed.
//! The same pattern works with Embassy on embedded targets.
//!
//! # Usage
//!
//! ```bash
//! # Terminal 1: Start zenoh router
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Terminal 2: Start the service server
//! cargo run -p native-rs-service-server
//!
//! # Terminal 3: Run this async client
//! cargo run -p native-rs-async-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_log::{Logger, nros_error, nros_info};

// RMW selection is build/config, never application logic (RFC-0031): the backend
// is the one `nros-rmw-*` optional dep activated by the config-lowered
// `rmw-{zenoh,xrce,cyclonedds}` feature. The `#[used]` static(s) below are a pure
// LINK-FORCE — they reference the backend's `register` symbol so the rlib's
// linkme `RMW_INIT_ENTRIES` self-register section is pulled into the link graph
// (rlib archive linking drops unreferenced objects, so this reference is
// required, NOT a `register()` call). The cffi walker in `nros::init` then
// discovers + registers the backend. Accepted link-force pattern (cf.
// `extern crate nros_platform_cffi as _`), not an RMW leak.
#[cfg(feature = "rmw-zenoh")]
#[used]
static __FORCE_LINK_ZENOH: fn() -> Result<(), nros_rmw_zenoh::RegisterError> =
    nros_rmw_zenoh::register;
#[cfg(feature = "rmw-xrce")]
#[used]
static __FORCE_LINK_XRCE: fn() -> Result<(), nros_rmw_xrce_cffi::RegisterError> =
    nros_rmw_xrce_cffi::register;
#[cfg(feature = "rmw-cyclonedds")]
#[used]
static __FORCE_LINK_CYCLONEDDS_SYS: fn() -> Result<(), nros_rmw_cyclonedds_sys::RegisterError> =
    nros_rmw_cyclonedds_sys::register;

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-client-async");

extern crate nros_platform_cffi as _;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(
        &LOGGER,
        "nros Async Service Client Example (tokio background spin)"
    );
    nros_info!(
        &LOGGER,
        "=========================================================="
    );

    // Create executor
    let config = ExecutorConfig::from_env().node_name("async_service_client");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    // Create client — it's an owned type (no lifetime tied to node or executor).
    // After this block, the node is dropped and the executor is free to move.
    let mut client = {
        let mut node = executor
            .create_node("async_service_client")
            .expect("Failed to create node");
        node.create_client::<AddTwoInts>("/add_two_ints")
            .expect("Failed to create client")
    };

    nros_info!(&LOGGER, "Service client created for: /add_two_ints");
    nros_info!(&LOGGER, "Using tokio background spin pattern");

    // LocalSet enables spawn_local (single-threaded, no Send bound needed)
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            // Spawn spin_async() as a background task on the same thread.
            // This drives I/O so service replies can arrive.
            tokio::task::spawn_local(async move {
                executor.spin_async().await;
            });

            // Sequential service calls — just .await the Promise directly.
            // The background spin task drives I/O concurrently.
            let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

            for (a, b) in test_cases {
                let request = AddTwoIntsRequest { a, b };
                nros_info!(&LOGGER, "Calling service: {} + {} = ?", a, b);

                let reply = match client.call(&request) {
                    Ok(promise) => match promise.await {
                        Ok(reply) => reply,
                        Err(e) => {
                            nros_error!(&LOGGER, "Service call failed: {:?}", e);
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        nros_error!(&LOGGER, "Failed to send request: {:?}", e);
                        std::process::exit(1);
                    }
                };

                nros_info!(&LOGGER, "Response: {} + {} = {}", a, b, reply.sum);
                assert_eq!(reply.sum, a + b, "Sum mismatch!");

                // Brief pause between calls
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }

            nros_info!(&LOGGER, "All async service calls completed successfully!");
        })
        .await;
}
