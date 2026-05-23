//! Native talker example — shared logic for both build paths.
//!
//! Phase 118 collapsed the per-RMW talkers into one crate; Phase 170.A
//! adds the Cyclone DDS build path. The crate exposes two entry points
//! over the same `run()` body:
//!   - `fn main()` (`src/main.rs`) — the pure-`cargo build` path for the
//!     `rmw-zenoh` / `rmw-xrce` features.
//!   - `rust_main()` (below, `#[no_mangle]`) — the C entry the cyclonedds
//!     `CMakeLists.txt` links into (Cyclone needs cmake-time idlc
//!     descriptors + the C++ backend, so `cargo build` alone can't link
//!     it; the staticlib crate-type lets cmake drive the link).

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

// Phase 118 — RMW selection is build-time via the mutually
// exclusive `rmw-{zenoh,cyclonedds,xrce}` features.
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

fn register_rmw() -> Result<(), &'static str> {
    // Phase 128.B.1 — the one-line `register()` doubles as the
    // (idempotent) backend registration AND the symbol reference that
    // drags the backend rlib's CGU into the binary on stable Rust.
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

/// Per-RMW default locator. Overridden at runtime via `NROS_LOCATOR`
/// (then `ZENOH_LOCATOR` for zenoh back-compat; ignored on Cyclone/XRCE).
fn default_locator() -> &'static str {
    #[cfg(feature = "rmw-zenoh")]
    {
        "tcp/127.0.0.1:7447"
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        "" // brokerless RTPS — locator ignored
    }
    #[cfg(feature = "rmw-xrce")]
    {
        "127.0.0.1:2019"
    }
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

/// Talker body — register the backend, open the executor, publish
/// `std_msgs/Int32` on `/chatter` every second. Shared by `main()` and
/// the C `rust_main()` entry.
pub fn run() {
    info!("nros Native Talker ({} Transport)", ACTIVE_RMW_NAME);
    info!("=========================================");

    register_rmw().expect("Failed to register RMW backend");

    let locator = std::env::var("NROS_LOCATOR")
        .or_else(|_| std::env::var("ZENOH_LOCATOR"))
        .unwrap_or_else(|_| default_locator().to_string());
    let domain_id = std::env::var("ROS_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let config = ExecutorConfig::new(&locator)
        .node_name("talker")
        .domain_id(domain_id);
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

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

/// C entry point for the cyclonedds CMake build path (`src/cyclonedds_main.c`
/// calls this from its `main`). Initializes logging then runs the talker.
#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    env_logger::init();
    run();
    0
}

// Pull the POSIX C platform port into the staticlib link graph so the
// cyclonedds CMake build resolves `nros_platform_*`.
#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_platform_cffi as _;
