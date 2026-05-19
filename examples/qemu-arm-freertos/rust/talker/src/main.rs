//! FreeRTOS QEMU Talker (Phase 118 collapsed)
//!
//! Publishes `std_msgs/Int32` messages on `/chatter`. RMW selected at
//! build time via mutually exclusive `rmw-{zenoh,dds}` Cargo features;
//! source body stays RMW-agnostic.

#![no_std]
#![no_main]

#[cfg(feature = "rmw-dds")]
extern crate alloc;
// Phase 121.9 — DDS path needs the `critical_section::Impl`
// registration that the shim provides; without this `extern crate`
// `--gc-sections` strips its static `set_impl!` invocation.
#[cfg(feature = "rmw-dds")]
extern crate nros_platform_critical_section as _;

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-dds")))]
compile_error!(
    "this FreeRTOS talker requires exactly one of `rmw-zenoh` or `rmw-dds`",
);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-dds")]
    {
        nros_rmw_dds::register().map_err(|_| "dds register failed")?;
    }
    Ok(())
}

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        // DDS uses domain_id only — locator string is ignored, the
        // RTPS port set is derived from the domain id.
        #[cfg(feature = "rmw-zenoh")]
        let locator = config.zenoh_locator;
        #[cfg(feature = "rmw-dds")]
        let locator = "";
        let exec_config = ExecutorConfig::new(locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
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

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
