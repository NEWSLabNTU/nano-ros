//! Serial (UART) listener example for QEMU MPS2-AN385
//!
//! Subscribes to Int32 messages over a zenoh serial transport using CMSDK UART0.
//! QEMU exposes UART0 as a host PTY (`-serial pty`), which can be connected
//! to zenohd's serial listener for bridging to the zenoh network.
//!
//! Run with:
//! ```sh
//! cargo run --release
//! ```
//! QEMU will print the PTY path (e.g., `/dev/pts/3`). Connect zenohd:
//! ```sh
//! zenohd --listen serial//dev/pts/3#baudrate=115200 --no-multicast-scouting
//! ```

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
            println!("Zenoh locator: {}", config.zenoh_locator);

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("serial_listener");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("serial_listener")?;

            println!("Subscribing to /chatter (std_msgs/Int32)");
            let mut subscription = node.create_subscription::<Int32>("/chatter")?;
            println!("Subscriber declared");

            println!("Waiting for messages over serial...");

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
