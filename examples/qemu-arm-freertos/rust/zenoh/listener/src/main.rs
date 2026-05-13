//! FreeRTOS QEMU Listener
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("listener");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("listener")?;

            println!("Subscribing to /chatter (std_msgs/Int32)");
            executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                println!("Received: {}", msg.data);
            })?;

            println!("Subscriber declared");
            println!("Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
