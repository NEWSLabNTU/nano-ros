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

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("listener-rtic");

extern crate nros_platform_cffi as _;

fn main() {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens). RMW selection is build/config, never source.
    nros_board_native::register_linked_rmw();

    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros RTIC-pattern Listener (native)");

    let config = ExecutorConfig::from_env().node_name("listener");
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
