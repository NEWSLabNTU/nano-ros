//! nros Zephyr Async Service Client (Rust) — Phase 168.3 collapsed shape.
//!
//! Embassy-driven async pattern: nros executor + service client are created
//! once, then the executor is moved to a background `spin_task`. Main task
//! awaits Promises directly. Two RMWs supported: zenoh + dds (xrce path
//! upstream missing async hooks).

#![no_std]

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-dds")))]
compile_error!("Exactly one rmw-* feature must be enabled (rmw-zenoh | rmw-dds).");

#[cfg(all(feature = "rmw-zenoh", feature = "rmw-dds"))]
compile_error!("rmw-zenoh and rmw-dds are mutually exclusive.");

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use static_cell::StaticCell;

type NrosExecutor = nros::Executor;

static EMBASSY: StaticCell<zephyr::embassy::Executor> = StaticCell::new();

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    { nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?; }
    #[cfg(feature = "rmw-dds")]
    { nros_rmw_dds::register().map_err(|_| "dds register failed")?; }
    Ok(())
}

#[cfg(feature = "rmw-zenoh")]
fn make_config() -> nros::ExecutorConfig<'static> {
    nros::ExecutorConfig::new("tcp/127.0.0.1:7466")
}

#[cfg(feature = "rmw-dds")]
fn make_config() -> nros::ExecutorConfig<'static> {
    nros::ExecutorConfig::new("")
        .domain_id(0)
        .node_name("dds_async_service_client")
}

#[no_mangle]
extern "C" fn rust_main() {
    unsafe { zephyr::set_logger().ok(); }
    info!("nros Zephyr Async Service Client (Embassy)");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    let executor = EMBASSY.init(zephyr::embassy::Executor::new());
    executor.run(|spawner| {
        spawner.spawn(app_main(spawner)).unwrap();
    });
}

// Zenoh path: zpico has its own kernel thread draining the wire, so
// `spin_async` blocks-when-idle without starving I/O.
#[cfg(feature = "rmw-zenoh")]
#[embassy_executor::task]
async fn spin_task(mut exec: NrosExecutor) -> ! {
    exec.spin_async().await
}

// DDS path: no kernel-side I/O pump. `spin_async` would idle forever
// when its drive() returns Quiescent. Poll `spin_once` on a timer
// (Phase 160.B follow-up).
#[cfg(feature = "rmw-dds")]
const SPIN_TICK_MS: u64 = 5;
#[cfg(feature = "rmw-dds")]
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

    let config = make_config();
    register_rmw().expect("Failed to register RMW backend");

    let mut nros_exec = nros::Executor::open(&config)?;

    let mut client = {
        let mut node = nros_exec.create_node("async_service_client")?;
        node.create_client::<AddTwoInts>("/add_two_ints")?
    };

    spawner.spawn(spin_task(nros_exec)).unwrap();

    info!("Async service client ready: /add_two_ints");

    // DDS needs longer discovery warmup; zenoh stabilises quickly.
    #[cfg(feature = "rmw-zenoh")]
    { embassy_time::Timer::after_secs(3).await; }
    #[cfg(feature = "rmw-dds")]
    { embassy_time::Timer::after_secs(10).await; }

    let test_cases = [(5i64, 3), (10, 20), (100, 200), (-5, 10)];

    for (a, b) in test_cases {
        let req = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);

        #[cfg(feature = "rmw-zenoh")]
        let reply = client.call(&req)?.await?;
        #[cfg(feature = "rmw-dds")]
        let reply = client
            .call(&req)?
            .poll_until_ready(|| embassy_time::Timer::after_millis(SPIN_TICK_MS))
            .await?;

        info!("Response: {} + {} = {}", a, b, reply.sum);
        embassy_time::Timer::after_millis(500).await;
    }

    info!("All async service calls completed!");

    loop {
        embassy_time::Timer::after_secs(60).await;
    }
}
