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

/// Polling cadence for the background spin task. Short enough that
/// dust-dds's UDP sockets don't accumulate (avoiding GEM RX
/// "alloc failed" on Cortex-A9), long enough that the embassy-time
/// driver gets a chance to schedule other tasks.
const SPIN_TICK_MS: u64 = 10;

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

/// Background task that drives nros I/O forever.
///
/// `Executor::spin_async()` parks indefinitely once the runtime
/// reports no pending work, with no way for arriving UDP packets
/// to wake it (the cooperative `NrosPlatformRuntime` has no socket
/// → embassy-waker bridge). For zenoh that doesn't matter — zpico
/// has its own kernel thread draining the wire — but for the DDS
/// backend `spin_async` is the sole I/O driver. We replace it with
/// `spin_once` on an embassy-time pacing loop so the task keeps
/// polling the runtime until process exit.
#[embassy_executor::task]
async fn spin_task(mut exec: NrosExecutor) -> ! {
    loop {
        let _ = exec.spin_once(core::time::Duration::from_millis(0));
        embassy_time::Timer::after_millis(SPIN_TICK_MS).await;
    }
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

    // Allow time for SPDP/SEDP discovery to complete. Uses
    // `embassy_time::Timer` (async) so the Embassy executor stays
    // free to schedule `spin_task` during the wait. A synchronous
    // `zephyr::time::sleep` here would park the whole single-threaded
    // executor, starve the I/O pump, and deadlock the discovery
    // handshake (Phase 71.29 follow-up).
    embassy_time::Timer::after_secs(10).await;

    let test_cases = [(5i64, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let req = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);

        // `nros-rmw-dds` now implements `register_waker` (Phase 71.29
        // follow-up): a `DataReaderListener` attached to the reply
        // reader fires `on_data_available` from dust-dds's task pool
        // when the reply lands, which wakes the future polling this
        // Promise.
        let reply = client.call(&req)?.await?;
        info!("Response: {} + {} = {}", a, b, reply.sum);

        embassy_time::Timer::after_millis(500).await;
    }

    info!("All async service calls completed!");

    loop {
        embassy_time::Timer::after_secs(60).await;
    }
}
