//! ThreadX QEMU RISC-V Listener
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_threadx_qemu_riscv64::{Config, println, run};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("listener");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("listener")?;

        println!("Subscribing to /chatter (std_msgs/Int32)");
        let mut subscription = node.create_subscription::<Int32>("/chatter")?;

        println!("Subscriber declared");
        println!("Waiting for messages...");

        loop {
            executor.spin_once(10);

            if let Some(msg) = subscription.try_recv()? {
                println!("Received: {}", msg.data);
            }
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
