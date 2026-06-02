//! Serial (UART) listener example for QEMU MPS2-AN385
//!
//! Subscribes to Int32 messages over a zenoh serial transport using CMSDK UART0.
//! QEMU exposes UART0 as a host PTY (`-serial pty`), which can be connected
//! to zenohd's serial listener for bridging to the zenoh network.
//!
//! Run with:
//! ```sh
//! cargo run --release
//! ```
//! QEMU will print the PTY path (e.g., `/dev/pts/3`). Connect zenohd:
//! ```sh
//! zenohd --listen serial//dev/pts/3#baudrate=115200 --no-multicast-scouting
//! ```

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385::{Config, run};
use nros_log::{Logger, nros_info};

// Phase 88.16.C — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("serial-listener");
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[nros_board_mps2_an385::entry]
fn main() -> ! {
    run(Config::serial_default(), |config| {
        nros_log::register_logger(&LOGGER);
        nros_log::init(nros_log::sinks::default());

        nros_info!(&LOGGER, "Zenoh locator: {}", config.zenoh_locator);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("serial_listener");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. On `target_os = "none"` the
        // `linkme` `RMW_INIT_ENTRIES` slice is an empty stub (Phase 142),
        // so this call is the ONLY reference that keeps the backend linked;
        // without it `--gc-sections` strips the whole zenoh backend and
        // `Executor::open` resolves `NoBackend`. (Verified Phase 204.1.)
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let nid = executor.node_builder("serial_listener").build()?;

        nros_info!(&LOGGER, "Subscribing to /chatter (std_msgs/Int32)");
        executor
            .node_mut(nid)
            .create_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                nros_info!(&LOGGER, "Received: {}", msg.data);
            })?;
        nros_info!(&LOGGER, "Subscriber declared");

        nros_info!(&LOGGER, "Waiting for messages over serial...");

        loop {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
