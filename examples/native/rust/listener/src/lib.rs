//! Native listener example — shared logic for both build paths.
//!
//! Phase 170.A — the standard listener body lives in `run()`, shared by
//! the pure-cargo `fn main()` (`src/main.rs`, zenoh/xrce) and the
//! Cyclone DDS `rust_main()` C entry (cmake links the C++ backend +
//! idlc descriptors, which `cargo build` alone can't). The safety-e2e
//! variant is zenoh-specific and stays in `main.rs`.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!(
    "examples/native/rust/listener requires exactly one of \
     `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce` to be enabled.",
);

/// Register the build-selected RMW backend. Public so the safety-e2e
/// `main` (zenoh-only) can reuse it.
pub fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?;
    }
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    Ok(())
}

const ACTIVE_RMW_NAME: &str = if cfg!(feature = "rmw-zenoh") {
    "Zenoh"
} else if cfg!(feature = "rmw-cyclonedds") {
    "CycloneDDS"
} else if cfg!(feature = "rmw-xrce") {
    "XRCE-DDS"
} else {
    "(none)"
};

/// Standard listener — subscribe to `std_msgs/Int32` on `/chatter` and
/// log each message. Shared by `main()` and the C `rust_main()`.
pub fn run() {
    info!("nros Native Listener ({} Transport)", ACTIVE_RMW_NAME);
    info!("==========================================");

    register_rmw().expect("Failed to register RMW backend");
    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");
    executor
        .node_mut(nid)
        .subscription("/chatter")
        .typed::<Int32>()
        .message_info()
        .build(move |msg, info| {
            info!("Received: {}", msg.data);
            if let Some(info) = info {
                let gid = info.publisher_gid();
                log::trace!(
                    "seq={} gid={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} ",
                    info.publication_sequence_number(),
                    gid[0],
                    gid[1],
                    gid[2],
                    gid[3],
                    gid[4],
                    gid[5],
                    gid[6],
                    gid[7],
                );
            }
        })
        .expect("Failed to add subscription");
    info!("Subscriber created for topic: /chatter");
    info!("Waiting for Int32 messages on /chatter...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}

/// C entry for the cyclonedds CMake build path.
#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    env_logger::init();
    run();
    0
}

#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_platform_cffi as _;
