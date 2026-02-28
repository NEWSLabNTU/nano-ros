//! ThreadX QEMU RISC-V Service Server
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.

#![no_std]
#![no_main]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_threadx_qemu_riscv64::{Config, println, run};

#[unsafe(no_mangle)]
fn _start() -> ! {
    run(Config::default(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_server");
        let mut executor = Executor::<_, 4, 4096>::open(&exec_config)?;

        executor.add_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            println!("Request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })?;

        println!("Service server ready on /add_two_ints");
        println!("Waiting for requests...");

        // Spin for a bounded time (test automation)
        for _ in 0..50000u32 {
            executor.spin_once(10);
        }

        println!("Server shutting down.");
        Ok::<(), NodeError>(())
    })
}
