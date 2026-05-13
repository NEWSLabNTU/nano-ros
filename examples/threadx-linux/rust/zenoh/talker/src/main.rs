//! ThreadX Linux Talker
//!
//! Phase 122.4 — L2 timer-driven publisher. Publishes 10
//! `std_msgs/Int32` messages on `/chatter` at 1 Hz, then exits.

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use nros::prelude::*;
use nros_board_threadx_linux::{Config, run};
use std_msgs::msg::Int32;

static COUNT: AtomicI32 = AtomicI32::new(0);
static DONE: AtomicBool = AtomicBool::new(false);

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
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
        println!();
        println!("Publishing messages...");

        executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
            let i = COUNT.fetch_add(1, Ordering::Relaxed);
            if let Err(e) = publisher.publish(&Int32 { data: i }) {
                println!("Publish failed: {:?}", e);
            } else {
                println!("Published: {}", i);
            }
            if i + 1 >= 10 {
                DONE.store(true, Ordering::Release);
            }
        })?;

        while !DONE.load(Ordering::Acquire) {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        println!();
        println!("Done publishing 10 messages.");
        Ok::<(), NodeError>(())
    })
}
