//! NuttX QEMU ARM Service Server Example
//!
//! Demonstrates an AddTwoInts service server on `/add_two_ints`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
use nros_log::{Logger, nros_info};

// Phase 88.16.D — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-server");

fn main() {
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

            nros_info!(&LOGGER, "Registering service: /add_two_ints (AddTwoInts)");
            executor
                .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
                    let sum = request.a + request.b;
                    nros_info!(&LOGGER, "Request: {} + {} = {}", request.a, request.b, sum);
                    AddTwoIntsResponse { sum }
                })
                .expect("Failed to add service");
            nros_info!(&LOGGER, "Service server ready");
            nros_info!(&LOGGER, "");
            nros_info!(&LOGGER, "Waiting for requests...");

            // Spin for a bounded time (embedded test pattern)
            for _ in 0..10000 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            nros_info!(&LOGGER, "");
            nros_info!(&LOGGER, "Server timeout, exiting.");
            Ok::<(), NodeError>(())
        },
    )
}
