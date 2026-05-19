//! NuttX QEMU ARM Service Client Example
//!
//! Calls the AddTwoInts service on `/add_two_ints`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
use nros_log::{nros_error, nros_info, nros_warn, Logger};

// Phase 88.16.D — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-client");

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        nros_log::register_logger(&LOGGER);
        nros_log::init(nros_log::sinks::default());

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_client");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor: Executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("add_two_ints_client")?;

        nros_info!(&LOGGER, "Creating service client: /add_two_ints (AddTwoInts)");
        let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;
        nros_info!(&LOGGER, "Client created — waiting for server discovery...");

        // Race-3 fix: gate the first `call()` on liveliness-token discovery.
        // On a multi-threaded zenoh-pico backend (NuttX), the client and
        // server boot in parallel and the first request can otherwise race
        // the router-side propagation of the server's queryable.
        // `wait_for_service` issues a `z_liveliness_get` and lets the
        // executor cooperatively spin until either a matching token reports
        // back or the timeout expires.
        let server_seen = client.wait_for_service(
            &mut executor,
            core::time::Duration::from_secs(10),
        )?;
        if !server_seen {
            nros_warn!(&LOGGER, "Service /add_two_ints not visible after 10s — bailing");
            return Err(NodeError::Timeout);
        }
        nros_info!(&LOGGER, "Server discovered — sending requests");
        nros_info!(&LOGGER, "");

        let test_cases = [(5, 3), (10, 20), (100, 200), (-5, 10)];

        for (a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            nros_info!(&LOGGER, "Calling: {} + {} = ?", a, b);

            let mut promise = client.call(&request)?;
            let response = promise.wait(&mut executor, core::time::Duration::from_millis(5000))?;

            nros_info!(&LOGGER, "Response: {} + {} = {}", a, b, response.sum);
        }

        nros_info!(&LOGGER, "");
        nros_info!(&LOGGER, "All service calls completed.");
        Ok::<(), NodeError>(())
    })
}
