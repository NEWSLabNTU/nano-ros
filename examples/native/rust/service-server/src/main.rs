//! Native Service Server — Phase 212.L.2 Application pkg shape.
//!
//! Serves `example_interfaces/srv/AddTwoInts` on `/add_two_ints`.
//! Single-file `[[bin]]`: explicit [`nros::init_with_launch_auto`]
//! (Pattern 2) then a user-owned spin loop.
//!
//! ```bash
//! cargo run -p native-rs-service-server   # then native-rs-service-client
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
use nros::prelude::*;

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!("this example requires exactly one of `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce`",);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        // Phase 214.S.4 — symbol-drag for `-sys` rlib (see talker).
        nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?;
    }
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    Ok(())
}

fn main() {
    env_logger::init();
    info!("nros Service Server Example");
    info!("================================");

    register_rmw().expect("Failed to register RMW backend");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("add_two_ints_server");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");
    info!("Node created: add_two_ints_server");

    executor
        .register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            info!("Received request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })
        .expect("Failed to add service");
    info!("Service server created: /add_two_ints");
    info!("Waiting for service requests...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
