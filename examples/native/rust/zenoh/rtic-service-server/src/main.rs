//! Native RTIC-pattern Service Server
//!
//! Validates the RTIC service server pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `service.handle_request()` (manual polling)
//!
//! This is the native equivalent of `examples/stm32f4/rust/zenoh/rtic-service-server/`.

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::info;
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros RTIC-pattern Service Server (native)");

    let config = ExecutorConfig::from_env().node_name("add_server");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("add_server")
        .expect("Failed to create node");
    let mut service = node
        .create_service::<AddTwoInts>("/add_two_ints")
        .expect("Failed to create service");

    info!("Service server ready: /add_two_ints");
    info!("Waiting for requests (RTIC pattern)...");

    let mut handled = 0u32;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);

    while std::time::Instant::now() < deadline {
        executor.spin_once(0);

        match service.handle_request(|req| {
            let sum = req.a + req.b;
            info!("Request: {} + {} = {}", req.a, req.b, sum);
            AddTwoIntsResponse { sum }
        }) {
            Ok(true) => handled += 1,
            Ok(false) => {}
            Err(e) => log::error!("Service error: {:?}", e),
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    info!("Done. Handled {} requests", handled);
}
