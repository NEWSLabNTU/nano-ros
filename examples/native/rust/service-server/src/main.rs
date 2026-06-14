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

// Phase 248 C6d — board-LESS APP owns + force-links its selected backend rlib.
// The `nros` umbrella no longer carries `rmw-*`, so its `__FORCE_LINK_*` statics
// are inert here; this `#[used]` static keeps the backend rlib (and its linkme
// `RMW_INIT_ENTRIES` self-register section) in the link graph so the backend
// auto-registers on POSIX. Mirrors `packages/core/nros/src/lib.rs`.
#[cfg(feature = "rmw-zenoh")]
#[used]
static __FORCE_LINK_ZENOH: fn() -> Result<(), nros_rmw_zenoh::RegisterError> =
    nros_rmw_zenoh::register;
#[cfg(feature = "rmw-xrce")]
#[used]
static __FORCE_LINK_XRCE: fn() -> Result<(), nros_rmw_xrce_cffi::RegisterError> =
    nros_rmw_xrce_cffi::register;
#[cfg(feature = "rmw-cyclonedds")]
#[used]
static __FORCE_LINK_CYCLONEDDS_SYS: fn() -> Result<(), nros_rmw_cyclonedds_sys::RegisterError> =
    nros_rmw_cyclonedds_sys::register;

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!("this example requires exactly one of `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce`",);

// Phase 227.3 (unified RMW) — backend self-registers via nros's __FORCE_LINK_* + the cffi walker; no register() call.

fn main() {
    env_logger::init();
    info!("nros Service Server Example");
    info!("================================");

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
