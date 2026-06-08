//! Native RTIC-pattern Service Server
//!
//! Validates the RTIC service server pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `service.handle_request()` (manual polling)
//!
//! This is the native equivalent of `examples/stm32f4/rust/rtic-service-server/`.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_log::{Logger, nros_error, nros_info};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("service-server-rtic");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros RTIC-pattern Service Server (native)");

    let config = ExecutorConfig::from_env().node_name("add_server");
    // Phase 227.3 (unified RMW) — backend self-registers via nros's __FORCE_LINK_* + the cffi walker; no register() call.
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("add_server")
        .expect("Failed to create node");
    let mut service = node
        .create_service::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create service");

    nros_info!(&LOGGER, "Service server ready: /add_two_ints");
    nros_info!(&LOGGER, "Waiting for requests (RTIC pattern)...");

    let mut handled = 0u32;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

    while std::time::Instant::now() < deadline {
        executor.spin_once(core::time::Duration::from_millis(0));

        match service.handle_request(|req| {
            let sum = req.a + req.b;
            nros_info!(&LOGGER, "Request: {} + {} = {}", req.a, req.b, sum);
            AddTwoIntsResponse { sum }
        }) {
            Ok(true) => handled += 1,
            Ok(false) => {}
            Err(e) => nros_error!(&LOGGER, "Service error: {:?}", e),
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    nros_info!(&LOGGER, "Done. Handled {} requests", handled);
}
