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

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-client-async");

extern crate nros_platform_cffi as _;

// Phase 118 — RMW selection is build-time via the mutually exclusive
// `rmw-{zenoh,cyclonedds,xrce}` features.
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!(
    "service-client-async requires exactly one of `rmw-zenoh`, \
     `rmw-cyclonedds`, or `rmw-xrce` to be enabled.",
);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    // Phase 214.S.4.b — no explicit cyclonedds register call. See
    // talker for the link-keep-alive rationale (the
    // __FORCE_LINK_CYCLONEDDS_SYS static inside nros-node::
    // cyclonedds_register pins the -sys rlib so its linkme
    // self-register section fires inside nros::init).
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    Ok(())
}

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
    // Phase 115.L.5 — install zenoh-pico C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    register_rmw().expect("Failed to register RMW backend");
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
