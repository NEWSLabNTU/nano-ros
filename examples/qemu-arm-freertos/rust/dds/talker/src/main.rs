//! FreeRTOS QEMU DDS Talker (Phase 97.4.freertos).
//!
//! Publishes `std_msgs/Int32` messages on `/chatter` over the
//! brokerless DDS / RTPS backend (`rmw-dds`). No zenoh router or
//! XRCE-DDS agent involved — the FreeRTOS image speaks RTPS
//! directly to peer DDS participants on the same domain.
//!
//! Phase 97.1 prerequisites wire in via Cargo.toml features:
//!   * `nros-board-mps2-an385-freertos` with `default-features = false`
//!     drops zenoh-pico C transport + the `zpico_set_task_config`
//!     boot call. Generic FreeRTOS / lwIP / poll task init keep
//!     working unchanged.
//!   * `nros-platform/global-allocator` — `pvPortMalloc` /
//!     `vPortFree` as Rust's `#[global_allocator]`.
//!   * `nros-platform/critical-section` — Cortex-M PRIMASK as
//!     `critical_section::Impl`, resolving the
//!     `_critical_section_1_0_*` symbols dust-dds references.
//!   * `nros = ["rmw-dds", "platform-freertos"]` — `NrosPlatformRuntime`
//!     drives dust-dds cooperatively from the FreeRTOS app task.

#![no_std]
#![no_main]

extern crate alloc;
// Phase 121.9 — pull the `critical_section::Impl` registration into
// this binary. Without an explicit `extern crate`, Rust's
// `--gc-sections` link stage strips the shim crate's static
// `set_impl!` registration and dust-dds's `_critical_section_1_0_*`
// references stay unresolved.
extern crate nros_platform_critical_section as _;

use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        // DDS uses domain_id only — no locator string. Pass an
        // empty locator and let `nros-rmw-dds`'s
        // `NrosUdpTransportFactory::create_participant` derive the
        // RTPS port set from the domain id.
        let exec_config = ExecutorConfig::new("")
            .domain_id(config.domain_id)
            .node_name("dds_talker");
        // Phase 115.L.5 — install dust-dds C-vtable backend.
        let mut executor = Executor::open(&exec_config)?;
        let publisher = {
            let mut node = executor.create_node("dds_talker")?;
            println!("Declaring publisher on /chatter (std_msgs/Int32) over DDS");
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
