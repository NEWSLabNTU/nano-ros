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
    // Use listener config (different IP/MAC than talker)
    run(Config::listener(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("listener");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("listener")?;

        println!("Subscribing to /chatter (std_msgs/Int32)");
        let mut subscription = node.create_subscription::<Int32>("/chatter")?;

        println!("Subscriber declared");
        println!("");
        println!("Waiting for messages...");

        let mut msg_count = 0u32;
        let mut poll_count = 0u32;

        loop {
            executor.spin_once(10);

            if let Some(msg) = subscription.try_recv()? {
                msg_count += 1;
                println!("Received [{}]: {}", msg_count, msg.data);

                if msg_count >= 10 {
                    println!("");
                    println!("Received 10 messages.");
                    break;
                }
            }

            poll_count += 1;
            if poll_count > 100000 {
                println!("");
                println!("Timeout waiting for messages.");
                break;
            }
        }

        Ok::<(), NodeError>(())
    })
}
