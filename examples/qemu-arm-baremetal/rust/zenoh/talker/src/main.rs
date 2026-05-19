//! Simple QEMU Talker using nros-board-mps2-an385
//!
//! Phase 122.4 — publisher driven by `Executor::register_timer`
//! (L2 callback) instead of an explicit spin-loop. Publishes typed
//! `std_msgs/Int32` messages on `/chatter` once per second.
//!
//! Phase 88.16.C — user diagnostics route through `nros-log`. The
//! board crate's own banner output (`Application completed
//! successfully.`, etc.) keeps using `hprintln!` because it runs
//! either before sinks are installed or from `run()` after the user
//! closure returns.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385::{Config, run};
use nros_log::{Logger, nros_error, nros_info};
use panic_semihosting as _;
use std_msgs::msg::Int32;

static LOGGER: Logger = Logger::new("talker");

#[nros_board_mps2_an385::entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("talker");
            let mut executor = Executor::open(&exec_config)?;
            let publisher = {
                let mut node = executor.create_node("talker")?;
                nros_info!(&LOGGER, "Declaring publisher on /chatter (std_msgs/Int32)");
                node.create_publisher::<Int32>("/chatter")?
            };
            nros_info!(&LOGGER, "Publisher declared");

            let mut count: i32 = 0;
            executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
                match publisher.publish(&Int32 { data: count }) {
                    Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                    Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
                }
                count = count.wrapping_add(1);
            })?;

            nros_info!(&LOGGER, "Publishing messages...");
            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
