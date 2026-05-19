//! FreeRTOS QEMU Service Server
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.

#![no_std]
#![no_main]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, run};
use nros_log::{Logger, nros_error, nros_info};

// Phase 88.16.C — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-server");
use panic_semihosting as _;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("add_two_ints_server");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor: Executor = Executor::open(&exec_config)?;

            executor.register_service::<AddTwoInts, _>("/add_two_ints", |request| {
                let sum = request.a + request.b;
                nros_info!(&LOGGER, "Request: {} + {} = {}", request.a, request.b, sum);
                AddTwoIntsResponse { sum }
            })?;

            nros_info!(&LOGGER, "Service server ready on /add_two_ints");
            nros_info!(&LOGGER, "Waiting for requests...");

            // Spin for a bounded time (test automation)
            for _ in 0..50000u32 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            nros_info!(&LOGGER, "Server shutting down.");
            Ok::<(), NodeError>(())
        },
    )
}
