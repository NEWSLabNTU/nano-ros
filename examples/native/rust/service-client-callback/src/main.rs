//! Native Service Client — **callback** variant (RFC-0041 / Phase 239).
//!
//! Calls `example_interfaces/srv/AddTwoInts` a few times, but receives each
//! reply through a `create_client_with_callback` closure dispatched at
//! `spin_once` — the dual-mode alternative to the `Promise`-based
//! `service-client` example (rclcpp `async_send_request(req, cb)` analogue).
//!
//! ```bash
//! cargo run -p native-rs-service-server          # then this client
//! cargo run -p native-rs-service-client-callback
//! ```

use core::time::Duration;
use std::{cell::Cell, rc::Rc};

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;

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

fn run() -> i32 {
    info!("nros Service Client Example (callback)");
    info!("======================================");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_client_cb");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    // The callback API lives on the executor-path node accessor
    // (`node_mut`), which registers an arena entry the executor drains at spin.
    let nid = executor
        .node_builder("add_two_ints_client_cb")
        .build()
        .expect("Failed to create node");
    info!("Node created: add_two_ints_client_cb");

    // Replies are delivered to this closure at `spin_once` — no Promise poll.
    // `replies` counts deliveries so the request loop knows when each landed.
    let replies = Rc::new(Cell::new(0usize));
    let replies_cb = replies.clone();
    let mut client = executor
        .node_mut(nid)
        .create_client_with_callback::<AddTwoInts, _>("/add_two_ints", move |reply| {
            info!("Response (callback): sum = {}", reply.sum);
            replies_cb.set(replies_cb.get() + 1);
        })
        .expect("Failed to create callback client");
    info!("Callback service client created for: /add_two_ints");

    // Let discovery settle (the callback client has no `wait_for_service`).
    for _ in 0..20 {
        executor.spin_once(Duration::from_millis(50));
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let mut ok = 0;
    for (a, b) in [(5, 3), (10, 20), (100, 200), (-5, 10)] {
        let before = replies.get();
        info!("Calling service: {} + {} = ?", a, b);
        if let Err(e) = client.call(&AddTwoIntsRequest { a, b }) {
            error!("Failed to send request: {:?}", e);
            continue;
        }
        // Spin until the reply callback fires (or a 5 s budget elapses).
        let mut waited_ms = 0u64;
        loop {
            executor.spin_once(Duration::from_millis(50));
            if replies.get() > before {
                ok += 1;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            waited_ms += 20;
            if waited_ms >= 5000 {
                error!("Timed out waiting for reply to {} + {}", a, b);
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    info!("{}/4 callback service calls succeeded", ok);
    if ok > 0 { 0 } else { 1 }
}

fn main() {
    env_logger::init();
    std::process::exit(run());
}
