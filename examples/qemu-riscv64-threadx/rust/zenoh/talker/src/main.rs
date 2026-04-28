//! ThreadX QEMU RISC-V Talker
//!
//! Publishes `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_threadx_qemu_riscv64::{Config, println, run};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
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
            for _ in 0..100 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            match publisher.publish(&Int32 { data: count }) {
                Ok(()) => println!("Published: {}", count),
                Err(e) => println!("Publish failed: {:?}", e),
            }
            count = count.wrapping_add(1);
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
