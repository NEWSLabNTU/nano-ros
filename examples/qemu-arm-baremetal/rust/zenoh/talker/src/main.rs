//! Simple QEMU Talker using nros-board-mps2-an385
//!
//! Phase 122.4 — publisher driven by `Executor::register_timer`
//! (L2 callback) instead of an explicit spin-loop. Publishes typed
//! `std_msgs/Int32` messages on `/chatter` once per second.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[nros_board_mps2_an385::entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("talker");
            let mut executor = Executor::open(&exec_config)?;
            let publisher = {
                let mut node = executor.create_node("talker")?;
                println!("Declaring publisher on /chatter (std_msgs/Int32)");
                node.create_publisher::<Int32>("/chatter")?
            };
            println!("Publisher declared");

            let mut count: i32 = 0;
            executor.register_timer(
                nros::TimerDuration::from_millis(1000),
                move || {
                    match publisher.publish(&Int32 { data: count }) {
                        Ok(()) => println!("Published: {}", count),
                        Err(e) => println!("Publish failed: {:?}", e),
                    }
                    count = count.wrapping_add(1);
                },
            )?;

            println!("Publishing messages...");
            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
