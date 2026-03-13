//! Serial (UART) talker example for QEMU MPS2-AN385
//!
//! Publishes Int32 messages over a zenoh serial transport using CMSDK UART0.
//! QEMU exposes UART0 as a host PTY (`-serial pty`), which can be connected
//! to zenohd's serial plugin for bridging to the zenoh network.
//!
//! Run with:
//! ```sh
//! cargo run --release
//! ```
//! QEMU will print the PTY path (e.g., `/dev/pts/3`). Connect zenohd:
//! ```sh
//! zenohd --connect serial//dev/pts/3#baudrate=115200
//! ```

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_mps2_an385::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[nros_mps2_an385::entry]
fn main() -> ! {
    run(Config::serial_default(), |config| {
        println!("Zenoh locator: {}", config.zenoh_locator);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("serial_talker");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("serial_talker")?;

        println!("Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        println!("Publisher declared");

        println!("");
        println!("Publishing messages over serial...");

        for i in 0..10i32 {
            // Poll to process serial transport events
            for _ in 0..100u32 {
                executor.spin_once(10);
            }

            if let Err(e) = publisher.publish(&Int32 { data: i }) {
                println!("Publish failed: {:?}", e);
            } else {
                println!("Published: {}", i);
            }
        }

        println!("");
        println!("Done publishing 10 messages.");

        Ok::<(), NodeError>(())
    })
}
