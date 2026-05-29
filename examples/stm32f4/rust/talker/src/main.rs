//! nros STM32F4 Talker Example using BSP
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`,
//! compatible with ROS 2 nodes via rmw_zenoh.
//!
//! # Hardware
//!
//! - Board: NUCLEO-F429ZI (or similar STM32F4 with Ethernet)
//! - Connect Ethernet cable to the board's RJ45 port
//!
//! # Network Configuration
//!
//! Default (static IP):
//! - Device IP: 192.168.1.10/24
//! - Gateway: 192.168.1.1
//! - Zenoh Router: 192.168.1.1:7447
//!
//! # Building
//!
//! ```bash
//! cargo build --release
//! cargo run --release  # Uses probe-rs to flash
//! ```
//!
//! # Logging
//!
//! Phase 88.16.F — user diagnostics route through `nros-log`.
//! `nros-platform-stm32f4`'s `PlatformLog` impl forwards every
//! record to `defmt::{trace,debug,info,warn,error}!` so the
//! existing `defmt_rtt` + `probe-rs attach` workflow keeps working
//! unchanged.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

// defmt 0.3 requires a timestamp function in each binary crate
defmt::timestamp!("{=u64:us}", { 0 });

use nros::prelude::*;
use nros_board_stm32f4::prelude::*;
use nros_log::{Logger, nros_error, nros_info};
use std_msgs::msg::Int32;

/// Poll interval in milliseconds
const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(10);

static LOGGER: Logger = Logger::new("talker");

#[entry]
fn main() -> ! {
    run(Config::nucleo_f429zi(), |config| -> Result<(), NodeError> {
        nros_log::register_logger(&LOGGER);
        nros_log::init(nros_log::sinks::default());

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        // Phase 104.A / 204.1 — bare-metal callers must explicitly register
        // the RMW backend: on `target_os = "none"` the `linkme`
        // `RMW_INIT_ENTRIES` slice is an empty stub (Phase 142), so this is
        // the only reference keeping the backend linked. Without it
        // `--gc-sections` strips the zenoh backend and `Executor::open`
        // resolves `NoBackend`. (Latent bug found in Phase 204.1.)
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let publisher = {
            let mut node = executor.create_node("talker")?;
            nros_info!(
                &LOGGER,
                "Creating publisher for /chatter (std_msgs/Int32)..."
            );
            node.create_publisher::<Int32>("/chatter")?
        };

        nros_info!(&LOGGER, "Starting publish timer (1 Hz)...");
        let mut counter: i32 = 0;
        executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
            counter = counter.wrapping_add(1);
            match publisher.publish(&Int32 { data: counter }) {
                Ok(()) => nros_info!(&LOGGER, "Published: {}", counter),
                Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
            }
        })?;

        loop {
            executor.spin_once(POLL_INTERVAL);
        }
    })
}
