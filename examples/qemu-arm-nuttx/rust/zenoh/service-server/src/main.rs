//! NuttX QEMU ARM Service Server Example
//!
//! Demonstrates an AddTwoInts service server on `/add_two_ints`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_nuttx_qemu_arm::{Config, run};

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_server");
        let mut executor: Executor = Executor::open(&exec_config)?;

        println!("Registering service: /add_two_ints (AddTwoInts)");
        executor
            .add_service::<AddTwoInts, _>("/add_two_ints", |request| {
                let sum = request.a + request.b;
                println!("Request: {} + {} = {}", request.a, request.b, sum);
                AddTwoIntsResponse { sum }
            })
            .expect("Failed to add service");
        println!("Service server ready");
        println!();
        println!("Waiting for requests...");

        // Spin for a bounded time (embedded test pattern)
        for _ in 0..10000 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        println!();
        println!("Server timeout, exiting.");
        Ok::<(), NodeError>(())
    })
}
