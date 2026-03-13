//! nros Zephyr Async Service Client Example (Rust)
//!
//! Demonstrates the background spin pattern using Embassy executor with
//! Zephyr's kernel-backed waking (`executor-zephyr` feature):
//!
//! 1. Create nros executor and service client (client is an owned type)
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
/// Embassy `#[task]` doesn't support generics, so we name the type explicitly.
type NrosExecutor = nros::Executor;

/// Static storage for the Embassy executor (lives for the program lifetime).
static EMBASSY: StaticCell<zephyr::embassy::Executor> = StaticCell::new();

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Async Service Client (Embassy)");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    // executor-zephyr: blocks on k_sem_take(K_FOREVER) when idle
    let executor = EMBASSY.init(zephyr::embassy::Executor::new());
    executor.run(|spawner| {
        spawner.spawn(app_main(spawner)).unwrap();
    });
}

/// Background task that drives nros I/O forever.
#[embassy_executor::task]
async fn spin_task(mut exec: NrosExecutor) -> ! {
    exec.spin_async().await
}

/// Main application task — creates nros session, spawns background spin,
/// then makes sequential service calls by .awaiting Promises.
#[embassy_executor::task]
async fn app_main(spawner: embassy_executor::Spawner) {
    if let Err(e) = run_async(spawner).await {
        error!("Error: {:?}", e);
    }
}

async fn run_async(spawner: embassy_executor::Spawner) -> Result<(), nros::NodeError> {
    let config = nros::ExecutorConfig::new("tcp/192.0.2.2:7447");
    let mut nros_exec = nros::Executor::open(&config)?;

    // Create client — it's an owned type (no lifetime tied to node or executor).
    // After this block, the node is dropped and the executor is free to move.
    let mut client = {
        let mut node = nros_exec.create_node("async_service_client")?;
        node.create_client::<AddTwoInts>("/add_two_ints")?
    };

    // Spawn background spin task (same thread, cooperative scheduling).
    // The executor is moved here — only the client remains in this task.
    spawner.spawn(spin_task(nros_exec)).unwrap();

    info!("Async service client ready: /add_two_ints");

    // Allow time for zenoh connection to stabilize
    zephyr::time::sleep(zephyr::time::Duration::secs(3));

    // Sequential service calls — just .await the Promise directly.
    // The background spin task drives I/O concurrently via k_sem waking.
    let test_cases = [(5i64, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let req = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);

        let reply = client.call(&req)?.await?;
        info!("Response: {} + {} = {}", a, b, reply.sum);

        zephyr::time::sleep(zephyr::time::Duration::millis(500));
    }

    info!("All async service calls completed!");

    // Keep running so the spin task can handle any remaining I/O
    loop {
        zephyr::time::sleep(zephyr::time::Duration::secs(60));
    }
}
