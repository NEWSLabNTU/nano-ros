//! Native Service Client — **callback** variant.
//!
//! Calls `example_interfaces/srv/AddTwoInts` once, receiving the reply
//! through a `create_client_with_callback` closure dispatched at
//! `spin_once` — the dual-mode alternative to the `Promise`-based
//! `service-client` example (rclcpp `async_send_request(req, cb)` analogue).
//! The summands come from argv, defaulting to the official demo's `2 3`;
//! the reply is logged as `Result of add_two_ints: N`.
//!
//! ```bash
//! cargo run -p native-rs-service-server          # then this client
//! cargo run -p native-rs-service-client-callback -- 2 3
//! ```

use core::time::Duration;
use std::{cell::Cell, rc::Rc};

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::prelude::*;

fn run() -> i32 {
    // Summands from argv, defaulting to the official demo's `2 3`.
    let mut args = std::env::args().skip(1).filter_map(|s| s.parse().ok());
    let a: i64 = args.next().unwrap_or(2);
    let b: i64 = args.next().unwrap_or(3);

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_client_cb");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    // The callback API lives on the executor-path node accessor
    // (`node_mut`), which registers an arena entry the executor drains at spin.
    let nid = executor
        .node_builder("add_two_ints_client_cb")
        .build()
        .expect("Failed to create node");

    // The reply is delivered to this closure at `spin_once` — no Promise poll.
    // `replied` flags delivery so the wait loop below knows when it landed.
    let replied = Rc::new(Cell::new(false));
    let replied_cb = replied.clone();
    let mut client = executor
        .node_mut(nid)
        .create_client_with_callback::<AddTwoInts, _>("/add_two_ints", move |reply| {
            info!("Result of add_two_ints: {}", reply.sum);
            replied_cb.set(true);
        })
        .expect("Failed to create callback client");

    // Let discovery settle (the callback client has no `wait_for_service`).
    for _ in 0..20 {
        executor.spin_once(Duration::from_millis(50));
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // One request, then spin until the reply callback fires (5 s budget).
    if let Err(e) = client.call(&AddTwoIntsRequest { a, b }) {
        error!("Failed to send request: {:?}", e);
        return 1;
    }
    let mut waited_ms = 0u64;
    while !replied.get() {
        executor.spin_once(Duration::from_millis(50));
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited_ms += 20;
        if waited_ms >= 5000 {
            error!("Timed out waiting for reply to {} + {}", a, b);
            return 1;
        }
    }
    0
}

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    env_logger::init();
    std::process::exit(run());
}
