//! FreeRTOS QEMU DDS Listener (Phase 97.4.freertos).
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter` over the
//! brokerless DDS / RTPS backend (`rmw-dds`).

#![no_std]
#![no_main]

extern crate alloc;

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new("")
            .domain_id(config.domain_id)
            .node_name("dds_listener");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("dds_listener")?;

        println!("Subscribing to /chatter (std_msgs/Int32) over DDS");
        let mut subscription = node.create_subscription::<Int32>("/chatter")?;
        println!("Subscriber declared");
        println!("Waiting for messages...");

        loop {
            executor.spin_once(core::time::Duration::from_millis(10));
            if let Some(msg) = subscription.try_recv()? {
                println!("Received: {}", msg.data);
            }
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
