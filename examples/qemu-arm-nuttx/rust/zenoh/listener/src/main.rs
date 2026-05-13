//! NuttX QEMU ARM Listener Example
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
use std_msgs::msg::Int32;

fn main() {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("listener");
            // Phase 115.L.x — install C-vtable backend before session open.
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("listener")?;

            println!("Subscribing to /chatter (std_msgs/Int32)");
            executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                println!("Received: {}", msg.data);
            })?;
            println!("Subscriber declared");
            println!("Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
