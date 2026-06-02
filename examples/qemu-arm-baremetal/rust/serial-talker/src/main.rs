//! Serial (UART) talker example for QEMU MPS2-AN385
//!
//! Publishes Int32 messages over a zenoh serial transport using CMSDK UART0.
//! QEMU exposes UART0 as a host PTY (`-serial pty`), which can be connected
//! to zenohd's serial plugin for bridging to the zenoh network.
//!
//! Run with:
//! ```sh
//! cargo run --release
//! ```
//! QEMU will print the PTY path (e.g., `/dev/pts/3`). Connect zenohd:
//! ```sh
//! zenohd --connect serial//dev/pts/3#baudrate=115200
//! ```

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385::{Config, run};
use nros_log::{Logger, nros_error, nros_info};

// Phase 88.16.C — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("serial-talker");
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
            .node_name("serial_talker");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. On `target_os = "none"` the
        // `linkme` `RMW_INIT_ENTRIES` slice is an empty stub (Phase 142),
        // so this call is the ONLY reference that keeps the backend linked;
        // without it `--gc-sections` strips the whole zenoh backend and
        // `Executor::open` resolves `NoBackend`. (Verified Phase 204.1.)
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("serial_talker")?;

        nros_info!(&LOGGER, "Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        nros_info!(&LOGGER, "Publisher declared");

        nros_info!(&LOGGER, "Publishing messages over serial...");

        let mut count: i32 = 0;
        loop {
            match publisher.publish(&Int32 { data: count }) {
                Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
            }
            count = count.wrapping_add(1);

            // Poll to process serial transport events (~1s between publishes)
            for _ in 0..100u32 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
