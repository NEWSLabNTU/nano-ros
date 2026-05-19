//! FreeRTOS QEMU Listener
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, run};
use nros_log::{Logger, nros_error, nros_info};

// Phase 88.16.C — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("listener");
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("listener");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("listener")?;

            nros_info!(&LOGGER, "Subscribing to /chatter (std_msgs/Int32)");
            executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                nros_info!(&LOGGER, "Received: {}", msg.data);
            })?;

            nros_info!(&LOGGER, "Subscriber declared");
            nros_info!(&LOGGER, "Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
