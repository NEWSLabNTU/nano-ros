//! FreeRTOS QEMU DDS Listener (Phase 97.4.freertos).
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter` over the
//! brokerless DDS / RTPS backend (`rmw-dds`).

#![no_std]
#![no_main]

extern crate alloc;
// Phase 121.9 — see sibling talker for rationale.
extern crate nros_platform_critical_section as _;

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new("")
                .domain_id(config.domain_id)
                .node_name("dds_listener");
            // Phase 115.L.5 — install dust-dds C-vtable backend.
            nros_rmw_dds::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("dds_listener")?;

            println!("Subscribing to /chatter (std_msgs/Int32) over DDS");
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
