//! FreeRTOS QEMU Service Server
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.

#![no_std]
#![no_main]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_server");
        let mut executor: Executor = Executor::open(&exec_config)?;

        executor.add_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            println!("Request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })?;

        println!("Service server ready on /add_two_ints");
        println!("Waiting for requests...");

        // Spin for a bounded time (test automation)
        for _ in 0..50000u32 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        println!("Server shutting down.");
        Ok::<(), NodeError>(())
    })
}
