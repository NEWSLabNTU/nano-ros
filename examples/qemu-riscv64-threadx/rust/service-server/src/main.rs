//! ThreadX QEMU RISC-V Service Server
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.

#![no_std]
#![no_main]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_board_threadx_qemu_riscv64::{Config, println, run};
#[cfg(not(feature = "rmw-zenoh"))]
compile_error!("this example requires `rmw-zenoh`");

fn register_rmw() -> Result<(), &'static str> {
    nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")
}


/// Locator override (`NROS_LOCATOR`) baked at build time; `no_std` so the
/// runtime `env::var` path is unavailable. Default targets the QEMU
/// host-loopback zenohd at fixture port 7463.
const LOCATOR: &str = match option_env!("NROS_LOCATOR") {
    Some(v) => v,
    None => "tcp/10.0.2.2:7463",
};

// TODO(213.E): plumb a build-time override for `domain_id` (Kconfig-style)
// alongside the locator. Low priority — fixtures rarely vary the domain.
const DOMAIN_ID: u32 = 0;

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    run(Config { zenoh_locator: LOCATOR, domain_id: DOMAIN_ID, ..Default::default() }, |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_two_ints_server");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        register_rmw().expect("Failed to register RMW backend");
        let mut executor: Executor = Executor::open(&exec_config)?;

        executor.register_service::<AddTwoInts, _>("/add_two_ints", |request| {
            let sum = request.a + request.b;
            println!("Request: {} + {} = {}", request.a, request.b, sum);
            AddTwoIntsResponse { sum }
        })?;

        println!("Service server ready on /add_two_ints");
        println!("Waiting for requests...");

        // Spin for a bounded time (test automation)
        for _ in 0..50000u32 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        println!("Server shutting down.");
        Ok::<(), NodeError>(())
    })
}
