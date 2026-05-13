//! ThreadX Linux Listener
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter`.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use nros::prelude::*;
use nros_board_threadx_linux::{Config, run};
use std_msgs::msg::Int32;

static MSG_COUNT: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicBool = AtomicBool::new(false);

fn main() {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("listener");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("listener")?;

            println!("Subscribing to /chatter (std_msgs/Int32)");
            // Phase 122.4 — L2 callback path. Counter + stop flag live as
            // `static` atomics so the 'static-bound register_subscription
            // closure can reach them without lifetime gymnastics.
            executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                let n = MSG_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
                println!("Received [{}]: {}", n, msg.data);
                if n >= 10 {
                    println!();
                    println!("Received 10 messages.");
                    DONE.store(true, Ordering::Release);
                }
            })?;

            println!("Subscriber declared");
            println!();
            println!("Waiting for messages...");

            let mut poll_count = 0u32;
            while !DONE.load(Ordering::Acquire) {
                executor.spin_once(core::time::Duration::from_millis(10));
                poll_count += 1;
                if poll_count > 100000 {
                    println!();
                    println!("Timeout waiting for messages.");
                    break;
                }
            }

            Ok::<(), NodeError>(())
        },
    )
}
