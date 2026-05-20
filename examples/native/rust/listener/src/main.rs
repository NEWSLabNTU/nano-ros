//! Native Listener Example — pure-`cargo` entry point.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter`. The standard listener
//! body lives in `lib.rs::run()` (shared with the cyclonedds CMake
//! build path's `rust_main`). This binary is the `cargo build` /
//! `cargo run` entry for `rmw-zenoh` (default) and `rmw-xrce`; the
//! cyclonedds variant builds via this dir's `CMakeLists.txt`.
//!
//! ```bash
//! zenohd --listen tcp/127.0.0.1:7447
//! cargo run -p native-rs-listener
//! ```
//! Override the locator with `NROS_LOCATOR` (or legacy `ZENOH_LOCATOR`);
//! `RUST_LOG=debug` for debug logs.

/// Safety-e2e listener (zenoh-specific): validates CRC + tracks seq gaps.
#[cfg(feature = "safety-e2e")]
fn main() {
    use log::{error, info};
    use nros::prelude::*;
    use std_msgs::msg::Int32;

    env_logger::init();
    info!("nros Native Listener (Safety E2E)");
    native_rs_listener::register_rmw().expect("Failed to register RMW backend");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    let mut count: u64 = 0;
    executor
        .register_subscription_with_safety::<Int32, _>("/chatter", move |msg, status| {
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

/// Standard listener — shared `run()` from the lib.
#[cfg(not(feature = "safety-e2e"))]
fn main() {
    env_logger::init();
    native_rs_listener::run();
}
