//! Native RTIC-pattern Listener
//!
//! Validates the RTIC integration pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `subscription.try_recv()` (manual polling)
//!
//! This is the native equivalent of `examples/stm32f4/rust/rtic-listener/`.

use nros::prelude::*;
use nros_log::{Logger, nros_error, nros_info};
use std_msgs::msg::Int32;

// Phase 248 C6d — board-LESS APP owns + force-links its selected backend rlib.
// The `nros` umbrella no longer carries `rmw-*`, so its `__FORCE_LINK_*` statics
// are inert here; this `#[used]` static keeps the backend rlib (and its linkme
// `RMW_INIT_ENTRIES` self-register section) in the link graph so the backend
// auto-registers on POSIX. Mirrors `packages/core/nros/src/lib.rs`.
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
static LOGGER: Logger = Logger::new("listener-rtic");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros RTIC-pattern Listener (native)");

    let config = ExecutorConfig::from_env().node_name("listener");
    // Phase 227.3 (unified RMW) — no explicit `register()`. The RMW is declared
    // via the build feature (routed through the `nros` umbrella); `nros`'s
    // `#[used] __FORCE_LINK_*` static keeps the backend's self-register section
    // in the link graph, and it fires inside `Executor::open` via the cffi walker.
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("listener")
        .expect("Failed to create node");
    let mut subscription = node
        .create_subscription::<Int32>("/chatter")
        .expect("Failed to create subscription");

    nros_info!(
        &LOGGER,
        "Waiting for Int32 messages on /chatter (RTIC pattern)..."
    );

    loop {
        executor.spin_once(core::time::Duration::from_millis(0));

        match subscription.try_recv() {
            Ok(Some(msg)) => {
                nros_info!(&LOGGER, "Received: {}", msg.data);
            }
            Ok(None) => {}
            Err(e) => nros_error!(&LOGGER, "Receive error: {:?}", e),
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
