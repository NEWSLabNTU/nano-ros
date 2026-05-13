//! Phase 123.A.10 — minimal Rust publisher demonstrating Pattern A.
//!
//! Path-depends on the in-workspace nano-ros checkout. Publishes
//! std_msgs/Int32 on /chatter at 1 Hz. Pairs with pkg_c_talker +
//! pkg_cpp_listener for a mixed-language demo.

use log::info;
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();
    info!("pkg_rust_publisher — multi-package-workspace demo");

    let config = ExecutorConfig::from_env().node_name("pkg_rust_publisher");
    let mut executor: Executor = Executor::open(&config).expect("open session");

    let publisher = {
        let mut node = executor
            .create_node("pkg_rust_publisher")
            .expect("create node");
        node.create_publisher::<Int32>("/chatter")
            .expect("create publisher")
    };

    let mut count: i32 = 0;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            let msg = Int32 { data: count };
            if let Err(err) = publisher.publish(&msg) {
                eprintln!("publish failed: {err:?}");
            } else {
                info!("[pkg_rust_publisher] sent: {}", msg.data);
            }
            count += 1;
        })
        .expect("register timer");

    info!("[pkg_rust_publisher] publishing /chatter @ 1 Hz (Ctrl-C to exit)");
    executor.spin_blocking(SpinOptions::default()).expect("spin");
}
