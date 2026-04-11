//! Simple QEMU Talker using nros-mps2-an385
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_mps2_an385::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[nros_mps2_an385::entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("talker");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("talker")?;

            println!("Declaring publisher on /chatter (std_msgs/Int32)");
            let publisher = node.create_publisher::<Int32>("/chatter")?;
            println!("Publisher declared");

            println!("Publishing messages...");

            let mut count: i32 = 0;
            loop {
                // Poll to process network events (~1s between publishes)
                for _ in 0..100 {
                    executor.spin_once(10);
                }

                match publisher.publish(&Int32 { data: count }) {
                    Ok(()) => println!("Published: {}", count),
                    Err(e) => println!("Publish failed: {:?}", e),
                }
                count = count.wrapping_add(1);
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
