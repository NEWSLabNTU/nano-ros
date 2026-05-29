//! nros Zephyr Async Service Client (Rust) — Phase 168.3 collapsed shape.
//!
//! Embassy-driven async pattern: nros executor + service client are created
//! once, then the executor is moved to a background `spin_task`. Main task
//! awaits Promises directly. Only the zenoh RMW supports the async surface
//! today; DDS retired (Phase 169.4); XRCE upstream lacks async hooks.

#![no_std]

#[cfg(not(feature = "rmw-zenoh"))]
compile_error!("The rmw-zenoh feature must be enabled.");

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use static_cell::StaticCell;

type NrosExecutor = nros::Executor;

static EMBASSY: StaticCell<zephyr::embassy::Executor> = StaticCell::new();

fn register_rmw() -> Result<(), &'static str> {
    nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    Ok(())
}

fn make_config() -> nros::ExecutorConfig<'static> {
    nros::ExecutorConfig::new("tcp/127.0.0.1:7466")
}

#[no_mangle]
extern "C" fn rust_main() {
    // SAFETY: installs the logger once during single-threaded startup, before any logging call.
    unsafe { zephyr::set_logger().ok(); }
    info!("nros Zephyr Async Service Client (Embassy)");
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

    let config = make_config();
    register_rmw().expect("Failed to register RMW backend");

    let mut nros_exec = nros::Executor::open(&config)?;

    let mut client = {
        let mut node = nros_exec.create_node("async_service_client")?;
        node.create_client::<AddTwoInts>("/add_two_ints")?
    };

    spawner.spawn(spin_task(nros_exec)).unwrap();

    info!("Async service client ready: /add_two_ints");
    embassy_time::Timer::after_secs(3).await;

    let test_cases = [(5i64, 3), (10, 20), (100, 200), (-5, 10)];
    for (a, b) in test_cases {
        let req = AddTwoIntsRequest { a, b };
        info!("Calling service: {} + {} = ?", a, b);
        let reply = client.call(&req)?.await?;
        info!("Response: {} + {} = {}", a, b, reply.sum);
        embassy_time::Timer::after_millis(500).await;
    }

    info!("All async service calls completed!");
    loop {
        embassy_time::Timer::after_secs(60).await;
    }
}
