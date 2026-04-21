//! Simple QEMU Listener using nros-mps2-an385
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_mps2_an385::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[nros_mps2_an385::entry]
fn main() -> ! {
    // Load config from config.toml (different IP/MAC than talker)
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
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
                executor.spin_once(core::time::Duration::from_millis(10));

                if let Some(msg) = subscription.try_recv()? {
                    println!("Received: {}", msg.data);
                }
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
