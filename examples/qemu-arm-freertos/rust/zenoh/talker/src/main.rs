//! FreeRTOS QEMU Talker
//!
//! Publishes `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, run};
use nros_log::{Logger, nros_error, nros_info};

// Phase 88.16.C — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("talker");
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
                .node_name("talker");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let publisher = {
                let mut node = executor.create_node("talker")?;
                nros_info!(&LOGGER, "Declaring publisher on /chatter (std_msgs/Int32)");
                node.create_publisher::<Int32>("/chatter")?
            };
            nros_info!(&LOGGER, "Publisher declared");

            nros_info!(&LOGGER, "Publishing messages...");

            let mut count: i32 = 0;
            executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
                match publisher.publish(&Int32 { data: count }) {
                    Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                    Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
                }
                count = count.wrapping_add(1);
            })?;

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
