//! FreeRTOS QEMU listener shared logic.
//!
//! The pure-cargo path enters through `src/main.rs::_start()` and lets
//! the Rust board crate create the FreeRTOS application task. The
//! CycloneDDS path is CMake-driven so the C++ backend and idlc
//! descriptors can be linked; it enters through `app_main()` after the
//! checked-in C startup has already created the application task and
//! started networking.

#![no_std]

#[cfg(feature = "rmw-cyclonedds")]
extern crate alloc;
// CycloneDDS uses critical sections through the platform shim. Keep the
// registration object in the staticlib even with section GC enabled.
#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_platform_critical_section as _;

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-cyclonedds")))]
compile_error!("this FreeRTOS listener requires exactly one of `rmw-zenoh` or `rmw-cyclonedds`",);

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
    // CycloneDDS is brokerless RTPS; the locator string is ignored.
    #[cfg(feature = "rmw-zenoh")]
    let locator = config.zenoh_locator;
    #[cfg(feature = "rmw-cyclonedds")]
    let locator = "";

    let exec_config = ExecutorConfig::new(locator)
        .domain_id(config.domain_id)
        .node_name("listener");

    // Bare-metal targets do not walk POSIX-style constructor sections,
    // so examples register the active backend explicitly.
    register_rmw().expect("Failed to register RMW backend");

    println!("Opening executor");
    let mut executor = Executor::open(&exec_config)?;
    println!("Executor open");
    let _node = executor.create_node("listener")?;

    println!("Subscribing to /chatter (std_msgs/Int32)");
    executor.register_subscription::<Int32, _>("/chatter", |msg: &Int32| {
        println!("Received: {}", msg.data);
    })?;

    println!("Subscriber declared");
    println!("Waiting for messages...");

    loop {
        executor.spin_once(core::time::Duration::from_millis(10));
    }
}

/// Pure-cargo entry used by the existing zenoh fixture path.
pub fn start_from_reset() -> ! {
    run(Config::from_toml(include_str!("../nros.toml")), run_app)
}

/// C entry point used by the CMake/CycloneDDS staticlib path.
#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn app_main() -> ! {
    println!("Starting Rust CycloneDDS listener");
    let config = Config::from_toml(include_str!("../nros.toml"));
    if let Err(e) = run_app(&config) {
        println!("Application error: {:?}", e);
    }
    loop {}
}
