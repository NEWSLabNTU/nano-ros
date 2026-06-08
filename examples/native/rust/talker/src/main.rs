//! Native Talker Example — Phase 212.L.2 Application pkg shape.
//!
//! Publishes `std_msgs/Int32` on `/chatter` every 1 s using nros on
//! native x86. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] (Pattern 2 — picks up launch
//! overlay env vars from the environment) then a user-owned spin
//! loop.
//!
//! # Usage
//!
//! ```bash
//! # Zenoh path (default):
//! zenohd --listen tcp/127.0.0.1:7447 &
//! cargo run -p native-rs-talker
//!
//! # XRCE path:
//! cargo run -p native-rs-talker --no-default-features --features rmw-xrce
//!
//! # Cyclone DDS path (Phase 212.K pure-cargo):
//! cargo run -p native-rs-talker --no-default-features --features rmw-cyclonedds
//! ```
//!
//! Override the locator at runtime with `NROS_LOCATOR` (or the legacy
//! `ZENOH_LOCATOR`). Enable debug logs with `RUST_LOG=debug`.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

// Phase 118 — RMW selection is build-time via the mutually exclusive
// `rmw-{zenoh,cyclonedds,xrce}` features.
#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!(
    "examples/native/rust/talker requires exactly one of \
     `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce` to be enabled. \
     The default feature set picks `rmw-zenoh`; pass \
     `--no-default-features --features rmw-X` to switch.",
);

// Phase 227.3 (unified RMW) — no explicit `register()` calls. The RMW is
// declared via the build feature (`rmw-zenoh` / `rmw-xrce` / `rmw-cyclonedds`),
// which routes through the `nros` umbrella; `nros`'s `#[used] __FORCE_LINK_*`
// statics keep the selected backend's `RMW_INIT_ENTRIES` self-register section
// in the link graph, and it fires inside `nros::init` via the cffi-rmw walker.
// (Bare-metal targets, where linkme is unsupported, still call `register()`.)

const ACTIVE_RMW_NAME: &str = if cfg!(feature = "rmw-zenoh") {
    "Zenoh"
} else if cfg!(feature = "rmw-cyclonedds") {
    "CycloneDDS"
} else if cfg!(feature = "rmw-xrce") {
    "XRCE-DDS"
} else {
    "(none)"
};

fn main() {
    env_logger::init();
    info!("nros Native Talker ({} Transport)", ACTIVE_RMW_NAME);
    info!("=========================================");

    // Phase 212.L.5 Pattern 2 — launch-aware init. Picks up
    // `ROS_DOMAIN_ID` / `NROS_LOCATOR` / `NROS_SESSION_MODE` /
    // `RMW_IMPLEMENTATION` from the environment, otherwise falls
    // back to the standard env-var defaults.
    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("talker");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    #[cfg(feature = "param-services")]
    {
        executor
            .register_parameter_services()
            .expect("Failed to register parameter services");
        executor.declare_parameter("start_value", ParameterValue::Integer(0));
        info!("Parameter services registered for /talker");
    }

    let publisher = {
        let mut node = executor
            .create_node("talker")
            .expect("Failed to create node");
        info!("Node created: talker");
        let pub_ = node
            .create_publisher::<Int32>("/chatter")
            .expect("Failed to create publisher");
        info!("Publisher created for topic: /chatter");
        pub_
    };

    #[cfg(feature = "param-services")]
    let counter_start = {
        let v = executor.get_parameter_integer("start_value").unwrap_or(0) as i32;
        info!("Counter start value: {}", v);
        v
    };
    #[cfg(not(feature = "param-services"))]
    let counter_start = 0i32;

    #[cfg(feature = "rmw-xrce")]
    {
        let mut count: i32 = counter_start;
        info!("Publishing Int32 messages every 1s...");
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let msg = Int32 { data: count };
            match publisher.publish(&msg) {
                Ok(()) => info!("Published: {}", count),
                Err(e) => error!("Publish error: {:?}", e),
            }
            count = count.wrapping_add(1);
            let _ = executor.spin_once(core::time::Duration::from_millis(10));
        }
    }

    #[cfg(not(feature = "rmw-xrce"))]
    {
        let mut count: i32 = counter_start;
        executor
            .register_timer(nros::TimerDuration::from_millis(1000), move || {
                let msg = Int32 { data: count };
                match publisher.publish(&msg) {
                    Ok(()) => info!("Published: {}", count),
                    Err(e) => error!("Publish error: {:?}", e),
                }
                count = count.wrapping_add(1);
            })
            .expect("Failed to register publish timer");
        info!("Publishing Int32 messages every 1s...");

        executor
            .spin_blocking(SpinOptions::default())
            .expect("spin_blocking error");
    }
}
