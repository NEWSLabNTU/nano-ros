//! ThreadX QEMU RISC-V talker shared logic.
//!
//! The pure-cargo path enters through `src/main.rs::main()` and lets the
//! Rust board crate start ThreadX. The CycloneDDS path is CMake-driven so
//! Cyclone's C++ backend and generated descriptors can be linked; it
//! enters through `app_main()` after the checked-in C startup has already
//! created the application task and started networking.

#![no_std]

#[cfg(feature = "rmw-cyclonedds")]
extern crate alloc;
#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_platform_critical_section as _;

use nros::prelude::*;
use nros_board_threadx_qemu_riscv64::{Config, println, run};
use std_msgs::msg::Int32;

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-cyclonedds")))]
compile_error!("this example requires `rmw-zenoh` or `rmw-cyclonedds`");

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?;
    }
    Ok(())
}

fn run_app(config: &Config) -> Result<(), NodeError> {
    #[cfg(feature = "rmw-zenoh")]
    let locator = config.zenoh_locator;
    #[cfg(feature = "rmw-cyclonedds")]
    let locator = "";

    let exec_config = ExecutorConfig::new(locator)
        .domain_id(config.domain_id)
        .node_name("talker");

    // Bare-metal targets do not walk POSIX-style constructor sections,
    // so examples register the active backend explicitly.
    register_rmw().expect("Failed to register RMW backend");

    let mut executor = Executor::open(&exec_config)?;
    let publisher = {
        let mut node = executor.create_node("talker")?;
        println!("Declaring publisher on /chatter (std_msgs/Int32)");
        node.create_publisher::<Int32>("/chatter")?
    };
    println!("Publisher declared");

    println!("Publishing messages...");

    let mut count: i32 = 0;
    executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
        match publisher.publish(&Int32 { data: count }) {
            Ok(()) => println!("Published: {}", count),
            Err(e) => println!("Publish failed: {:?}", e),
        }
        count = count.wrapping_add(1);
    })?;

    loop {
        executor.spin_once(core::time::Duration::from_millis(10));
    }
}

/// Locator override (`NROS_LOCATOR`) baked at build time; `no_std` so the
/// runtime `env::var` path is unavailable. Default targets the QEMU
/// host-loopback zenohd at fixture port 7553.
const LOCATOR: &str = match option_env!("NROS_LOCATOR") {
    Some(v) => v,
    None => "tcp/10.0.2.2:7553",
};

// TODO(213.E): plumb a build-time override for `domain_id` (Kconfig-style)
// alongside the locator. Low priority — fixtures rarely vary the domain.
const DOMAIN_ID: u32 = 0;

/// Pure-cargo entry used by the existing zenoh fixture path.
pub fn start_from_reset() -> ! {
    run(Config { zenoh_locator: LOCATOR, domain_id: DOMAIN_ID, ..Default::default() }, run_app)
}

/// C entry point used by the CMake/CycloneDDS staticlib path.
#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn app_main() -> ! {
    println!("Starting Rust CycloneDDS talker");
    let config = Config { zenoh_locator: LOCATOR, domain_id: DOMAIN_ID, ..Default::default() };
    if let Err(e) = run_app(&config) {
        println!("Application error: {:?}", e);
    }
    loop {}
}
