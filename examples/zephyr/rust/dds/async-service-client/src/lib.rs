//! nros Zephyr DDS Async Service Client Example (Rust)
//!
//! Demonstrates the background spin pattern using Embassy executor with
//! Zephyr's kernel-backed waking (`executor-zephyr` feature) over the
//! DDS-RTPS RMW backend.
//!
//! 1. Create nros executor and service client
//! 2. Move executor to a background `spin_async()` Embassy task
//! 3. `.await` Promises directly from the main task
//!
//! The Embassy executor uses `k_sem_take`/`k_sem_give` for proper kernel
//! sleeping — no busy-looping. Single-threaded cooperative concurrency.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use static_cell::StaticCell;

/// Concrete nros executor type for Embassy task signatures.
type NrosExecutor = nros::Executor;

/// Static storage for the Embassy executor (lives for the program lifetime).
static EMBASSY: StaticCell<zephyr::embassy::Executor> = StaticCell::new();

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr DDS Async Service Client (Embassy)");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    let executor = EMBASSY.init(zephyr::embassy::Executor::new());
    executor.run(|spawner| {
        spawner.spawn(app_main(spawner)).unwrap();
    });
}

#[embassy_executor::task]
async fn spin_task(mut exec: NrosExecutor) -> ! {
    exec.spin_async().await
}

#[embassy_executor::task]
async fn app_main(spawner: embassy_executor::Spawner) {
    if let Err(e) = run_async(spawner).await {
        error!("Error: {:?}", e);
    }
}

async fn run_async(spawner: embassy_executor::Spawner) -> Result<(), nros::NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);

    let config = nros::ExecutorConfig::new("")
        .domain_id(0)
        .node_name("dds_async_service_client");
    let mut nros_exec = nros::Executor::open(&config)?;

    let mut client = {
        let mut node = nros_exec.create_node("async_service_client")?;
        node.create_client::<AddTwoInts>("/add_two_ints")?
    };

    spawner.spawn(spin_task(nros_exec)).unwrap();

    info!("Async service client ready: /add_two_ints");

    // Allow time for SPDP/SEDP discovery to complete
    zephyr::time::sleep(zephyr::time::Duration::secs(3));

    let test_cases = [(5i64, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let req = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);

        let reply = client.call(&req)?.await?;
        info!("Response: {} + {} = {}", a, b, reply.sum);

        zephyr::time::sleep(zephyr::time::Duration::millis(500));
    }

    info!("All async service calls completed!");

    loop {
        zephyr::time::sleep(zephyr::time::Duration::secs(60));
    }
}
