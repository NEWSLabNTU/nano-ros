//! Imperative E2E-safety listener (cross-process subscriber fixture).
//!
//! Moved out of `examples/native/rust/listener` (it was the `safety-e2e`
//! cfg-gated second `main`) so the example stays cfg-free. Subscribes to
//! `std_msgs/Int32` on `/chatter` through the imperative builder's
//! `.safety()` opt-in and logs the per-message integrity status
//! (`[SAFETY] seq_gap=.. dup=.. crc=..`).
//!
//! Paired with `tests/safety_e2e.rs`: the safety talker (publisher, attaches
//! CRC + sequence) runs in a separate process over zenohd; this subscriber
//! validates the CRC end-to-end (zenoh-pico does not deliver in-process, so
//! the topology is cross-process). The declarative (`Node` + `.safety()`)
//! sibling is `bins/declarative-safety-listener`.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();
    // Zenoh-only fixture: register the backend explicitly (the examples route
    // this through `nros_board_native::register_linked_rmw()`).
    nros_rmw_zenoh::register().expect("register zenoh backend");

    info!("nros Native Listener (Safety E2E)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");
    let mut count: u64 = 0;
    executor
        .node_mut(nid)
        .subscription("/chatter")
        .typed::<Int32>()
        .safety()
        .build(move |msg, status| {
            count += 1;
            let crc_str = match status.crc_valid {
                Some(true) => "ok",
                Some(false) => "FAIL",
                None => "n/a",
            };
            info!(
                "[{}] Received: data={} [SAFETY] seq_gap={} dup={} crc={}",
                count, msg.data, status.gap, status.duplicate, crc_str
            );
        })
        .expect("Failed to add safety subscription");
    info!("Safety subscriber created for topic: /chatter");
    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
